use super::*;

#[test]
fn renderer_command_actions_include_generated_title_rename() {
    /*
    CDXC:AgentSkills 2026-06-17-17:02:
    Rust gxserver must accept the same renderer `renameCommand` action as the TypeScript daemon so a full cutover keeps Claude Code generated-title renames on the native Enter path.
    */
    assert!(RENDERER_COMMAND_ACTIONS.contains(&"renameCommand"));
}

#[test]
fn renderer_command_actions_keep_only_renderer_owned_mobile_timer_actions() {
    /*
    CDXC:DelayedSend 2026-08-17:
    Delayed Send now enters first-class daemon endpoints. Keeping the old
    renderer actions would arm a second timer in whichever desktop client
    happened to be connected.
    */
    assert!(!RENDERER_COMMAND_ACTIONS.contains(&"scheduleDelayedSend"));
    assert!(!RENDERER_COMMAND_ACTIONS.contains(&"cancelDelayedSend"));
    assert!(RENDERER_COMMAND_ACTIONS.contains(&"toggleCloseAfterDone"));
}

#[test]
fn renderer_command_payload_adds_structured_session_target() {
    /*
    CDXC:CefRuntime 2026-06-21-19:22:
    Rust gxserver must normalize renderer-command payloads from any client so macOS receives a project-scoped session target and does not have to match raw G ids against combined sidebar presentation ids.
    */
    let payload = Map::from_iter([
        ("globalRef".to_string(), json!("S90:P1a:G9a")),
        ("projectId".to_string(), json!("P1a")),
        ("sessionId".to_string(), json!("G9a")),
        ("title".to_string(), json!("GPUI Sidebar Resize Parity")),
    ]);

    let normalized = with_renderer_session_target(payload);

    assert_eq!(
        normalized.get("sessionTarget"),
        Some(&json!({
            "globalRef": "S90:P1a:G9a",
            "projectId": "P1a",
            "sessionId": "G9a",
        }))
    );
    assert_eq!(normalized.get("sessionId"), Some(&json!("G9a")));
}

#[test]
fn first_prompt_auto_title_decides_provider_strategy_and_filters_meta_prompts() {
    let codex = json!({
        "agentId": "codex",
        "runtimeSettings": {},
        "title": "Codex Session",
    });
    let decision = decide_first_prompt_auto_title(
        &codex,
        Some("Please can you help me fix flaky tests."),
        false,
    );
    assert!(!decision.should_run);
    assert_eq!(
        decision.normalized_prompt.as_deref(),
        Some("fix flaky tests")
    );
    assert_eq!(decision.reason, "agentAutoTitle");
    assert_eq!(decision.strategy, Some("agentAutoTitle"));

    let claude = json!({
        "agentId": "claude",
        "runtimeSettings": {},
        "title": "Claude Code",
    });
    let decision = decide_first_prompt_auto_title(&claude, Some("Summarize the logs"), false);
    assert!(!decision.should_run);
    assert_eq!(decision.strategy, Some("agentAutoTitle"));

    let meta = decide_first_prompt_auto_title(&codex, Some("# AGENTS.md instructions"), false);
    assert!(!meta.should_run);
    assert_eq!(meta.reason, "metaPrompt");

    let slash = decide_first_prompt_auto_title(
        &codex,
        Some("notes before command\n  /status please"),
        false,
    );
    assert!(!slash.should_run);
    assert_eq!(slash.reason, "slashCommand");
}

#[test]
fn agent_session_title_command_uses_provider_specific_slash_command() {
    assert_eq!(
        agent_session_title_command(Some("hermes-agent"), "Investigate hooks"),
        "/title Investigate hooks"
    );
    assert_eq!(
        agent_session_title_command(Some("Hermes Agent"), "Investigate hooks"),
        "/title Investigate hooks"
    );
    assert_eq!(
        agent_session_title_command(Some("pi"), "Investigate hooks"),
        "/name Investigate hooks"
    );
    assert_eq!(
        agent_session_title_command(Some("codex"), "Investigate hooks"),
        "/rename Investigate hooks"
    );
}

#[test]
fn requested_agent_title_command_submission_requires_opt_in_and_agent_rename() {
    let mut params = Map::new();
    params.insert("submitAgentRenameCommand".to_string(), Value::Bool(true));
    params.insert("title".to_string(), json!("Investigate hooks"));
    let result = json!({
        "session": {
            "agentId": "hermes-agent",
            "projectId": "P1abc",
            "sessionId": "G1abc"
        },
        "shouldSendAgentRenameCommand": true
    });

    assert_eq!(
        requested_agent_title_command_submission("/api/requestSessionRename", &params, &result),
        Some((
            "P1abc".to_string(),
            "G1abc".to_string(),
            "/title Investigate hooks".to_string()
        ))
    );

    params.remove("submitAgentRenameCommand");
    assert_eq!(
        requested_agent_title_command_submission("/api/requestSessionRename", &params, &result),
        None
    );
}

#[test]
fn first_prompt_auto_title_attempt_rejects_stale_same_prompt_job() {
    let session = json!({
        "runtimeSettings": {
            "firstUserMessage": "Please fix the sidebar",
            "gxserverFirstPromptAutoTitleAttemptId": "replacement-attempt",
            "gxserverFirstPromptAutoTitleStatus": "running"
        }
    });

    assert!(is_current_first_prompt_auto_title_attempt_for_prompt(
        &session,
        "replacement-attempt",
        Some("fix the sidebar")
    ));
    assert!(!is_current_first_prompt_auto_title_attempt_for_prompt(
        &session,
        "cancelled-attempt",
        Some("fix the sidebar")
    ));
}

#[test]
fn generated_first_prompt_titles_are_sanitized_and_clamped() {
    let title = parse_generated_session_title_text(
        "```text\n\"Investigate Sidebar Resize Regression With Extra Words\"\n```",
    )
    .expect("title");
    assert_eq!(title, "Investigate Sidebar Resize Regression");
    assert!(js_string_length(&title) <= GXSERVER_GENERATED_SESSION_TITLE_MAX_LENGTH);
}

#[test]
fn first_prompt_title_caps_use_javascript_utf16_length() {
    let rocket = "\u{1F680}";
    let exact = js_string_slice_prefix(
        &rocket.repeat(126),
        GXSERVER_FIRST_PROMPT_TITLE_SOURCE_MAX_LENGTH,
    );
    assert_eq!(exact, rocket.repeat(125));
    assert_eq!(
        js_string_length(&exact),
        GXSERVER_FIRST_PROMPT_TITLE_SOURCE_MAX_LENGTH
    );

    let split = js_string_slice_prefix(
        &format!(
            "{}{}",
            "a".repeat(GXSERVER_FIRST_PROMPT_TITLE_SOURCE_MAX_LENGTH - 1),
            rocket
        ),
        GXSERVER_FIRST_PROMPT_TITLE_SOURCE_MAX_LENGTH,
    );
    assert_eq!(
        split,
        format!(
            "{}{}",
            "a".repeat(GXSERVER_FIRST_PROMPT_TITLE_SOURCE_MAX_LENGTH - 1),
            char::REPLACEMENT_CHARACTER
        )
    );
    assert_eq!(
        js_string_length(&split),
        GXSERVER_FIRST_PROMPT_TITLE_SOURCE_MAX_LENGTH
    );
}

#[test]
fn generated_title_clamp_counts_non_bmp_as_javascript_utf16() {
    let rocket = "\u{1F680}";
    let title = parse_generated_session_title_text(&rocket.repeat(20)).expect("title");
    assert_eq!(
        title,
        format!("{}{}", rocket.repeat(19), char::REPLACEMENT_CHARACTER)
    );
    assert_eq!(
        js_string_length(&title),
        GXSERVER_GENERATED_SESSION_TITLE_MAX_LENGTH
    );
}
