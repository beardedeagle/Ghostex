use serde_json::{json, Value};

use super::*;
use crate::domain::DomainRepository;

#[test]
fn terminal_title_capture_preserves_decision_reason_when_identity_title_is_trusted() {
    let (_temp, db) = open_test_database();
    let repository = DomainRepository::new(&db, "test-server");
    let project = repository
        .create_project(
            json!({ "name": "Terminal Title Capture", "path": std::env::temp_dir() })
                .as_object()
                .expect("project params"),
        )
        .expect("create project");
    let project_id = project
        .get("projectId")
        .and_then(Value::as_str)
        .expect("project id")
        .to_string();
    let session = repository
        .create_session(
            json!({
                "agentId": "codex",
                "kind": "agent",
                "projectId": project_id,
                "runtimeSettings": {
                    "agentName": "codex",
                    "agentSessionId": "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb",
                    "titleSource": "user"
                },
                "title": "Phase 6 Ingested Title"
            })
            .as_object()
            .expect("session params"),
            false,
        )
        .expect("create session");
    let lifecycle = LifecycleParams {
        project_id: session
            .get("projectId")
            .and_then(Value::as_str)
            .expect("session project id")
            .to_string(),
        session_id: session
            .get("sessionId")
            .and_then(Value::as_str)
            .expect("session id")
            .to_string(),
    };
    let captured_id = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa";

    let result = ingest_terminal_title_event(
        &repository,
        &lifecycle,
        json!({
            "agentName": "codex",
            "rawTitle": captured_id,
            "sessionPersistenceProvider": "zmx"
        })
        .as_object()
        .expect("terminal title params"),
    )
    .expect("terminal title result");
    let result = result.result;

    assert_eq!(result.get("changed"), Some(&json!(true)));
    assert_eq!(
        result.get("reason"),
        Some(&json!("captured-agent-session-id"))
    );
    assert_eq!(result.get("agentSessionId"), Some(&json!(captured_id)));
    let session = result.get("session").expect("result session");
    assert_eq!(session.get("title"), Some(&json!("Phase 6 Ingested Title")));
    assert_eq!(
        session
            .get("runtimeSettings")
            .and_then(Value::as_object)
            .and_then(|settings| settings.get("agentSessionId")),
        Some(&json!(captured_id))
    );
}

#[test]
fn terminal_title_applies_zmx_title_with_previous_source_reason_without_agent_promotion() {
    let (_temp, db) = open_test_database();
    let repository = DomainRepository::new(&db, "test-server");
    let project = repository
        .create_project(
            json!({ "name": "Terminal Title Canonical", "path": std::env::temp_dir() })
                .as_object()
                .expect("project params"),
        )
        .expect("create project");
    let project_id = project
        .get("projectId")
        .and_then(Value::as_str)
        .expect("project id")
        .to_string();
    let session = repository
        .create_session(
            json!({
                "kind": "terminal",
                "projectId": project_id,
                "runtimeSettings": { "titleSource": "placeholder" },
                "title": "Search by Text"
            })
            .as_object()
            .expect("session params"),
            false,
        )
        .expect("create session");
    let lifecycle = LifecycleParams {
        project_id,
        session_id: session
            .get("sessionId")
            .and_then(Value::as_str)
            .expect("session id")
            .to_string(),
    };

    let output = ingest_terminal_title_event(
        &repository,
        &lifecycle,
        json!({
            "rawTitle": "Find previous Codex work",
            "sessionPersistenceProvider": "zmx"
        })
        .as_object()
        .expect("terminal title params"),
    )
    .expect("terminal title result");
    let result = output.result;

    assert!(output.schedule_presentation_delta);
    assert_eq!(result.get("changed"), Some(&json!(true)));
    assert_eq!(
        result.get("reason"),
        Some(&json!("zmx-terminal-title-from-placeholder"))
    );
    let session = result.get("session").expect("result session");
    assert_eq!(session.get("kind"), Some(&json!("terminal")));
    assert_eq!(session.get("agentId"), None);
    assert_eq!(
        session.get("title"),
        Some(&json!("Find previous Codex work"))
    );
    assert_eq!(
        session
            .get("runtimeSettings")
            .and_then(Value::as_object)
            .and_then(|settings| settings.get("agentName")),
        None
    );
}

#[test]
fn terminal_title_strips_factory_droid_status_marker_before_sync() {
    let (_temp, db) = open_test_database();
    let repository = DomainRepository::new(&db, "test-server");
    let project = repository
        .create_project(
            json!({ "name": "Factory Droid Titles", "path": std::env::temp_dir() })
                .as_object()
                .expect("project params"),
        )
        .expect("create project");
    let project_id = project
        .get("projectId")
        .and_then(Value::as_str)
        .expect("project id")
        .to_string();
    let session = repository
        .create_session(
            json!({
                "agentId": "droid",
                "kind": "agent",
                "projectId": project_id,
                "runtimeSettings": {
                    "agentName": "factory droid",
                    "titleSource": "placeholder"
                },
                "title": "Factory Droid Session"
            })
            .as_object()
            .expect("session params"),
            false,
        )
        .expect("create session");
    let lifecycle = LifecycleParams {
        project_id,
        session_id: session
            .get("sessionId")
            .and_then(Value::as_str)
            .expect("session id")
            .to_string(),
    };

    let output = ingest_terminal_title_event(
        &repository,
        &lifecycle,
        json!({
            "agentName": "factory droid",
            "rawTitle": "\u{26ec} New Session",
            "sessionPersistenceProvider": "zmx"
        })
        .as_object()
        .expect("terminal title params"),
    )
    .expect("terminal title result");
    let result = output.result;

    assert!(output.schedule_presentation_delta);
    assert_eq!(result.get("changed"), Some(&json!(true)));
    assert_eq!(
        result.get("reason"),
        Some(&json!("zmx-terminal-title-from-placeholder"))
    );
    let session = result.get("session").expect("result session");
    assert_eq!(session.get("title"), Some(&json!("New Session")));
    assert_eq!(
        session
            .get("runtimeSettings")
            .and_then(Value::as_object)
            .and_then(|settings| settings.get("titleSource")),
        Some(&json!("terminal-auto"))
    );
}

#[test]
fn terminal_title_rejects_untrusted_provider_off_title() {
    let (_temp, db) = open_test_database();
    let repository = DomainRepository::new(&db, "test-server");
    let project = repository
        .create_project(
            json!({ "name": "Terminal Title Trust", "path": std::env::temp_dir() })
                .as_object()
                .expect("project params"),
        )
        .expect("create project");
    let project_id = project
        .get("projectId")
        .and_then(Value::as_str)
        .expect("project id")
        .to_string();
    let session = repository
        .create_session(
            json!({
                "kind": "terminal",
                "projectId": project_id,
                "runtimeSettings": { "titleSource": "user" },
                "title": "Terminal Session"
            })
            .as_object()
            .expect("session params"),
            false,
        )
        .expect("create session");
    let lifecycle = LifecycleParams {
        project_id,
        session_id: session
            .get("sessionId")
            .and_then(Value::as_str)
            .expect("session id")
            .to_string(),
    };

    let output = ingest_terminal_title_event(
        &repository,
        &lifecycle,
        json!({
            "rawTitle": "Untrusted local shell title",
            "sessionPersistenceProvider": "off"
        })
        .as_object()
        .expect("terminal title params"),
    )
    .expect("terminal title result");
    let result = output.result;

    assert!(!output.schedule_presentation_delta);
    assert_eq!(result.get("changed"), Some(&json!(false)));
    assert_eq!(
        result.get("reason"),
        Some(&json!("terminal-title-not-trusted"))
    );
    assert_eq!(
        result
            .get("session")
            .and_then(|session| session.get("title")),
        Some(&json!("Terminal Session"))
    );
}

#[test]
fn terminal_title_status_bookkeeping_does_not_schedule_presentation_delta() {
    let (_temp, db) = open_test_database();
    let repository = DomainRepository::new(&db, "test-server");
    let project = repository
        .create_project(
            json!({ "name": "Terminal Title Status", "path": std::env::temp_dir() })
                .as_object()
                .expect("project params"),
        )
        .expect("create project");
    let project_id = project
        .get("projectId")
        .and_then(Value::as_str)
        .expect("project id")
        .to_string();
    let session = repository
        .create_session(
            json!({
                "kind": "terminal",
                "projectId": project_id,
                "runtimeSettings": { "titleSource": "terminal-auto" },
                "title": "Search by Text"
            })
            .as_object()
            .expect("session params"),
            false,
        )
        .expect("create session");
    let lifecycle = LifecycleParams {
        project_id,
        session_id: session
            .get("sessionId")
            .and_then(Value::as_str)
            .expect("session id")
            .to_string(),
    };

    let output = ingest_terminal_title_event(
        &repository,
        &lifecycle,
        json!({
            "rawTitle": "Search by Text",
            "sessionPersistenceProvider": "zmx"
        })
        .as_object()
        .expect("terminal title params"),
    )
    .expect("terminal title result");
    let result = output.result;

    assert!(!output.schedule_presentation_delta);
    assert_eq!(result.get("changed"), Some(&json!(false)));
    assert_eq!(
        result
            .get("activity")
            .and_then(|activity| activity.get("activity")),
        Some(&json!("idle"))
    );
    assert_eq!(
        result
            .get("session")
            .and_then(|session| session.get("runtimeSettings"))
            .and_then(Value::as_object)
            .and_then(|settings| settings.get("agentActivity"))
            .and_then(|activity| activity.get("lastTitle")),
        Some(&json!("Search by Text"))
    );
}
