//! Chat, Status Indicators, and the Tools group of General: Browser, Dev Servers, Editor and
//! File opening.
use super::super::super::catalog::SettingOption;
use super::super::super::catalog::{module, settings_catalog};
use super::super::super::fields::{ButtonVariant, settings_button};
use super::super::super::fields::{
    RowSpec, SELECT_WIDTH, ignored_ports_field, pet_picker_field, reset_key, segmented_field,
    setting_row, settings_section, settings_select_with_unavailable, text_field,
};
use super::super::super::page::PageBlock;
use super::super::orchestrator_instructions::{
    instructions_control, instructions_summary, open_instructions_sheet, summary_line,
};
use super::{GeneralCx, GeneralTab, save};
use gpui::{AnyElement, Context, SharedString, Window};
use serde_json::json;

fn chat_section(
    page: &mut GeneralTab,
    g: &GeneralCx,
    window: &mut Window,
    cx: &mut Context<GeneralTab>,
) -> Option<PageBlock> {
    if !g.search.section_visible("chat") {
        return None;
    }
    let s = "chat";
    let mut rows: Vec<AnyElement> = Vec::new();
    if g.visible(s, "preferredAgentInterface") {
        let spec = RowSpec::new("Default Agent View")
            .description("Chat switches on automatically as soon as Ghostex detects a compatible agent. The terminal stays live in the background, and you can switch back at any time. Settings > Agents can override this for one agent at a time.")
            .modified(g.values.is_modified("preferredAgentInterface"));
        rows.extend(page.segmented(
            g,
            s,
            "preferredAgentInterface",
            spec,
            Some(reset_key::<GeneralTab>("preferredAgentInterface")),
            g.options("PREFERRED_AGENT_INTERFACE_OPTIONS"),
            cx,
        ));
    }
    // CDXC:Sessions 2026-10-09 DECISION: the New-session cleanup is an Advanced setting, off by default, that only runs for Chat; gxserver enforces the Chat condition.
    rows.extend(page.toggle(g, s, "closeEmptySessionsOnNew", "Close empty sessions when starting a new one", "When you start a new session, close this project's other sessions that have nothing typed, drafted or queued. Only applies when Chat is the default view for that agent; with Terminal it never runs.", false, cx));
    if g.visible(s, "sessionChatFontFamily") {
        let spec = g.spec(
            "sessionChatFontFamily",
            "Font Family",
            "Type an installed font family name. Leave blank to use the app font.",
        );
        let value = g.values.string("sessionChatFontFamily");
        rows.push(text_field(
            page,
            &g.p,
            "sessionChatFontFamily",
            spec,
            Some(reset_key::<GeneralTab>("sessionChatFontFamily")),
            &value,
            Some("App default"),
            None,
            None,
            window,
            cx,
        ));
    }
    rows.extend(page.slider(
        g,
        s,
        "sessionChatZoomPercent",
        "Default Chat Zoom (%)",
        "Scale the desktop chat interface, including messages and the prompt composer, from 70% to 200% in 5% steps. Default: 100%.",
        (
            g.number("MIN_SESSION_CHAT_ZOOM_PERCENT"),
            g.number("MAX_SESSION_CHAT_ZOOM_PERCENT"),
            g.number("SESSION_CHAT_ZOOM_PERCENT_STEP"),
        ),
        false,
        window,
        cx,
    ));
    rows.extend(page.toggle(
        g,
        s,
        "sessionChatCustomTranscriptWidthEnabled",
        "Custom Transcript Width",
        "Let the transcript use a different width from the prompt composer.",
        false,
        cx,
    ));
    if g.values.bool("sessionChatCustomTranscriptWidthEnabled") {
        rows.extend(page.slider(
            g,
            s,
            "sessionChatTranscriptWidthPercent",
            "Transcript Width (%)",
            "Set the centered transcript width on wide panes. The prompt composer keeps its standard width.",
            (
                g.number("MIN_SESSION_CHAT_TRANSCRIPT_WIDTH_PERCENT"),
                g.number("MAX_SESSION_CHAT_TRANSCRIPT_WIDTH_PERCENT"),
                g.number("SESSION_CHAT_TRANSCRIPT_WIDTH_PERCENT_STEP"),
            ),
            true,
            window,
            cx,
        ));
    }
    rows.extend(page.toggle(g, s, "sessionChatFileEditPreviews", "Show file edit previews", "Show the first seven code lines in each file edit. Turn off to show only the path and change counts.", false, cx));
    rows.extend(page.toggle(g, s, "sessionChatMutedThinking", "Grey thinking", "Show the agent's thinking in grey and its replies in the full text colour, so the two are easy to tell apart.", false, cx));
    rows.extend(page.toggle(g, s, "sessionChatKeepComposerExpanded", "Keep chat box expanded while scrolling", "Keep the desktop chat box at full size while you scroll the transcript instead of shrinking it as you scroll up and growing it back at the end.", false, cx));
    rows.extend(page.toggle(g, s, "sessionChatConfirmEscapeInterrupt", "Press Escape twice to interrupt", "While the agent is working, the first Escape asks you to press Escape again within 2 seconds before it interrupts. Turn off to interrupt on the first Escape.", false, cx));
    rows.extend(page.toggle(g, s, "sessionChatVerboseMode", "Verbose Mode", "Expand thinking blocks to show their tool calls by default. Individual command and output details remain collapsible. This is the default for new chats; the Verbose pill in a chat's composer overrides it for that chat only.", false, cx));
    rows.extend(page.toggle(g, s, "sessionChatSimpleMode", "Simple mode", "Simplify all chats: hide tool command previews and group file edits behind an expandable file count.", false, cx));
    rows.extend(page.toggle(g, s, "chatCatchMeUp", "Catch me up button", "Show a Catch me up button at the top right of the chat box while it is empty. It has your Title Generation Agent (a small, cheap model) read the session's last 30 agent messages and shows where the session stands: what needs you, what is done, in progress or untested.", false, cx));
    rows.extend(page.toggle(g, s, "chatPreviousQuestion", "Previous question button", "Show a Previous question button next to Scroll to bottom while the chat has questions you haven't answered yet. Each click goes to the next older one and briefly outlines it.", false, cx));
    rows.extend(page.toggle(g, s, "chatSendOnPick", "Send when you pick an answer", "When a question card asks one question, picking an answer sends it right away, without Send answers. Cards with several questions, multiple-choice questions, Other and text answers still wait for Send.", false, cx));
    rows.extend(page.toggle(g, s, "markedHandovers", "Marked handovers in every session", "Orchestrators and their threads start each part of the message they hand back with a marker such as ❓ Needs you, ✅ Done, ⚠️ Read this, 🧪 Untested or 🛑 Blocked, so a quick skim shows what needs you. Turn this on to have every other agent session do the same. Applies to sessions started or resumed after you change it.", false, cx));
    rows.extend(page.marked_sections_style_row(g, s, window, cx));
    rows.extend(page.toggle(g, s, "orchestratorPlainReplies", "Orchestrators answer without markers", "Orchestrators answer normally, without marking the sections of their handovers. Their threads still mark the reports they send to the orchestrator. Applies to orchestrators started or resumed after you change it.", false, cx));
    rows.extend(orchestrator_instructions_row(page, g, s, cx));
    settings_section(&g.p, "Chat", None, None, rows)
        .map(|section| PageBlock::section("chat", section))
}

/// Orchestrator instructions for all projects: the first line of the text and the Edit button that
/// opens their editor (tabs/orchestrator_instructions.rs). The text lives in gxserver, not in the
/// settings file.
fn orchestrator_instructions_row(
    page: &mut GeneralTab,
    g: &GeneralCx,
    section: &str,
    cx: &mut Context<GeneralTab>,
) -> Option<AnyElement> {
    const KEY: &str = "orchestratorInstructions";
    if !g.visible(section, KEY) {
        return None;
    }
    let summary = instructions_summary(page, None, cx);
    let spec = RowSpec::new("Orchestrator instructions").description(
        "Standing instructions every new orchestrator starts with, so every thread it starts gets them too. A project can have its own in Settings > Projects, which also covers its worktrees. Orchestrators you already have keep their own.",
    );
    let edit = settings_button(
        &g.p,
        "orchestrator-instructions-edit",
        "Edit…",
        None,
        ButtonVariant::Outline,
        !page.store.read(cx).request().gxserver_rpc_available,
        None,
        |page: &mut GeneralTab, window, cx| open_instructions_sheet(page, None, window, cx),
        cx,
    );
    let control = instructions_control(&g.p, &summary_line(summary.as_ref(), false), edit);
    Some(setting_row(&g.p, KEY, spec, None, control, cx))
}

fn status_indicators_section(
    page: &mut GeneralTab,
    g: &GeneralCx,
    window: &mut Window,
    cx: &mut Context<GeneralTab>,
) -> Option<PageBlock> {
    if !settings_catalog().flag(module::PETS, "PET_CONTROLS_VISIBLE")
        || !g.search.section_visible("statusIndicators")
    {
        return None;
    }
    let s = "statusIndicators";
    let mut rows: Vec<AnyElement> = Vec::new();
    rows.extend(page.toggle(
        g,
        s,
        "petOverlayEnabled",
        "Wake Pet",
        "Show a draggable floating animated pet.",
        false,
        cx,
    ));
    if g.visible(s, "selectedPetId") {
        let spec = g.spec("selectedPetId", "Pet", "Choose the pet sprite.");
        let value = g.values.string("selectedPetId");
        rows.push(pet_picker_field(
            page,
            &g.p,
            spec,
            Some(reset_key::<GeneralTab>("selectedPetId")),
            &value,
            window,
            cx,
        ));
    }
    settings_section(&g.p, "Status Indicators", None, None, rows)
        .map(|section| PageBlock::section("statusIndicators", section))
}

fn browser_section(
    page: &mut GeneralTab,
    g: &GeneralCx,
    window: &mut Window,
    cx: &mut Context<GeneralTab>,
) -> Option<PageBlock> {
    if !g.search.subsection_visible("browser", g.power_visible) {
        return None;
    }
    let rows: Vec<AnyElement> = page
        .select(
            g,
            "browser",
            "webLinkOpenTarget",
            "Open links in",
            "Open web links from terminal output (Command-click), session chat, and detected dev servers in the project Browser view or the system default browser.",
            g.options("WEB_LINK_OPEN_TARGET_OPTIONS"),
            None,
            false,
            window,
            cx,
        )
        .into_iter()
        .collect();
    settings_section(&g.p, "Browser", None, None, rows)
        .map(|section| PageBlock::section("browser", section))
}

fn dev_servers_section(
    page: &mut GeneralTab,
    g: &GeneralCx,
    window: &mut Window,
    cx: &mut Context<GeneralTab>,
) -> Option<PageBlock> {
    if !g
        .search
        .subsection_visible("terminalDevServers", g.power_visible)
    {
        return None;
    }
    let s = "terminalDevServers";
    let mut rows: Vec<AnyElement> = Vec::new();
    rows.extend(page.toggle(
        g,
        s,
        "terminalDevServerDetectionEnabled",
        "Detect running servers in terminals",
        "Detect localhost dev server URLs from terminal output.",
        false,
        cx,
    ));
    if g.visible(s, "terminalDevServerIgnoredPortRules") {
        let spec = g.spec(
            "terminalDevServerIgnoredPortRules",
            "Ignored ports",
            "Servers on these ports are hidden from the server menu. Enter a port or an inclusive range.",
        );
        let value = g.values.value("terminalDevServerIgnoredPortRules");
        rows.push(ignored_ports_field(
            page,
            &g.p,
            spec,
            Some(reset_key::<GeneralTab>("terminalDevServerIgnoredPortRules")),
            &value,
            window,
            cx,
        ));
    }
    settings_section(
        &g.p,
        "Dev Servers",
        Some(SharedString::from(
            "Choose how Ghostex discovers running dev servers and which ports stay hidden. Detected URLs follow Browser \u{2192} Open links in.",
        )),
        None,
        rows,
    )
    .map(|section| PageBlock::section("terminalDevServers", section))
}

fn editor_section(
    page: &mut GeneralTab,
    g: &GeneralCx,
    cx: &mut Context<GeneralTab>,
) -> Option<PageBlock> {
    if !g.search.subsection_visible("editor", g.power_visible) {
        return None;
    }
    let s = "editor";
    let mut rows: Vec<AnyElement> = Vec::new();
    // CDXC:CodeEditor 2026-06-08-20:12: these two rows carry the advanced marker only, no reset.
    if g.visible(s, "codeServerLinkVscodeUserConfig") {
        let spec = RowSpec::new("Use VS Code settings")
            .description("Use local VS Code settings instead of the bundled editor defaults.")
            .advanced(settings_catalog().is_advanced("codeServerLinkVscodeUserConfig"));
        rows.push(super::super::super::fields::toggle_field_with(
            &g.p,
            "codeServerLinkVscodeUserConfig",
            spec,
            g.values.bool("codeServerLinkVscodeUserConfig"),
            None,
            |page: &mut GeneralTab, checked, _window, cx| {
                save(page, "codeServerLinkVscodeUserConfig", json!(checked), cx)
            },
            cx,
        ));
    }
    if g.values.bool("codeServerLinkVscodeUserConfig")
        && g.visible(s, "codeServerUseVscodeInsidersUserConfig")
    {
        let spec = RowSpec::new("Use VS Code Insiders settings")
            .description("Use the VS Code Insiders user settings directory.")
            .advanced(settings_catalog().is_advanced("codeServerUseVscodeInsidersUserConfig"))
            .dependent();
        rows.push(super::super::super::fields::toggle_field_with(
            &g.p,
            "codeServerUseVscodeInsidersUserConfig",
            spec,
            g.values.bool("codeServerUseVscodeInsidersUserConfig"),
            None,
            |page: &mut GeneralTab, checked, _window, cx| {
                save(
                    page,
                    "codeServerUseVscodeInsidersUserConfig",
                    json!(checked),
                    cx,
                )
            },
            cx,
        ));
    }
    rows.extend(page.toggle(
        g,
        s,
        "showUntrackedProjectDiffWhenNoTrackedChanges",
        "Show untracked lines without tracked changes",
        "When tracked git diff is +0 -0, show untracked line counts in project headers.",
        false,
        cx,
    ));
    settings_section(&g.p, "Editor", None, None, rows)
        .map(|section| PageBlock::section("editor", section))
}

/// The "As before" choice of HTML, Markdown and images: shown only while an older per-kind choice
/// still decides (`documentFileOpenTarget` unsaved and one of the old keys not at its default).
const DOCUMENT_FILE_OPEN_AS_BEFORE: &str = "asBefore";

/// What the old Markdown, HTML and Images settings still choose, in words, or `None` when they
/// are all at their defaults (everything in Files).
fn document_file_open_as_before(g: &GeneralCx) -> Option<String> {
    use ghostex_settings_catalog::file_opening::{
        DocumentFileKind, legacy_document_file_open_target,
    };
    if g.values.map().contains_key("documentFileOpenTarget") {
        return None;
    }
    let place = |kind| match legacy_document_file_open_target(g.values.map(), kind) {
        "code" => "Code",
        "system-app" => "the system app",
        _ => "Files",
    };
    let (markdown, html, images) = (
        place(DocumentFileKind::Markdown),
        place(DocumentFileKind::Html),
        place(DocumentFileKind::Image),
    );
    if [markdown, html, images]
        .iter()
        .all(|place| *place == "Files")
    {
        return None;
    }
    Some(format!(
        "As before: Markdown in {markdown}, HTML in {html}, images in {images}"
    ))
}

fn file_opening_section(
    page: &mut GeneralTab,
    g: &GeneralCx,
    window: &mut Window,
    cx: &mut Context<GeneralTab>,
) -> Option<PageBlock> {
    if !g.search.subsection_visible("fileOpening", g.power_visible) {
        return None;
    }
    let s = "fileOpening";
    // The editors this computer found (`workspaceOpenTargetAvailability`, the Open In page reads
    // the same list).
    let installed: Vec<String> = g
        .values
        .value("workspaceOpenTargetAvailability")
        .get("availableTargetIds")
        .and_then(serde_json::Value::as_array)
        .map(|ids| {
            ids.iter()
                .filter_map(serde_json::Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let mut targets: Vec<SettingOption> =
        ghostex_settings_catalog::file_opening::file_open_target_options()
            .into_iter()
            .map(|option| SettingOption {
                label: option.label,
                value: option.value,
            })
            .collect();
    let unavailable: Vec<String> = targets
        .iter()
        .map(|option| option.value.clone())
        .filter(|value| value != "files" && value != "code" && !installed.contains(value))
        .collect();
    for option in &mut targets {
        if unavailable.contains(&option.value) {
            option.label = format!("{} (not installed)", option.label);
        }
    }
    let mut rows: Vec<AnyElement> = Vec::new();
    for (key, label, subtitle) in [
        (
            "documentFileOpenTarget",
            "HTML, Markdown and images",
            ".html, .md and pictures such as .png, .jpg and .svg, opened from Quick Access, agent chat or the terminal.",
        ),
        (
            "otherFileOpenTarget",
            "Other files",
            "Code and text files, opened from Quick Access, agent chat or the terminal.",
        ),
    ] {
        if !g.visible(s, key) {
            continue;
        }
        let mut options = targets.clone();
        let as_before = (key == "documentFileOpenTarget")
            .then(|| document_file_open_as_before(g))
            .flatten();
        let value = match &as_before {
            Some(label) => {
                options.insert(
                    0,
                    SettingOption {
                        label: label.clone(),
                        value: DOCUMENT_FILE_OPEN_AS_BEFORE.to_string(),
                    },
                );
                DOCUMENT_FILE_OPEN_AS_BEFORE.to_string()
            }
            None => {
                let allowed: Vec<String> =
                    options.iter().map(|option| option.value.clone()).collect();
                g.values.choice(key, &allowed)
            }
        };
        let control = settings_select_with_unavailable(
            page,
            &g.p,
            key,
            &options,
            &unavailable,
            &value,
            Some(SELECT_WIDTH),
            false,
            None,
            move |page: &mut GeneralTab, next, _window, cx| {
                if next != DOCUMENT_FILE_OPEN_AS_BEFORE {
                    save(page, key, json!(next), cx);
                }
            },
            window,
            cx,
        );
        rows.push(setting_row(
            &g.p,
            key,
            RowSpec::new(label).subtitle(subtitle),
            None,
            control,
            cx,
        ));
    }
    let media_options = g.options("MEDIA_FILE_OPEN_TARGET_OPTIONS");
    rows.extend(
        [
            ("videoFileOpenTarget", "Videos", ".webm and .ogv play in Files. Formats Files cannot play, such as .mp4 and .mov, always open in the system app."),
            ("audioFileOpenTarget", "Audio", ".mp3, .wav, .ogg, .flac and .opus play in Files. Formats Files cannot play, such as .m4a, always open in the system app."),
        ]
        .into_iter()
        .filter(|(key, _, _)| g.visible(s, key))
        .map(|(key, label, subtitle)| {
            let allowed: Vec<String> = media_options
                .iter()
                .map(|option| option.value.clone())
                .collect();
            let value = g.values.choice(key, &allowed);
            // `MediaFileOpenTargetSetting`: a subtitle, no reset.
            segmented_field(
                &g.p,
                key,
                RowSpec::new(label).subtitle(subtitle),
                None,
                &media_options,
                Some(&value),
                None,
                move |page: &mut GeneralTab, next, _window, cx| save(page, key, json!(next), cx),
                cx,
            )
        }),
    );
    settings_section(
        &g.p,
        "File opening",
        Some(SharedString::from(
            "Choose where files open from Quick Access, agent chat links and terminal links. An editor app shows here once it is installed on this computer. If a view is unavailable, Ghostex uses the other one.",
        )),
        None,
        rows,
    )
    .map(|section| PageBlock::section("fileOpening", section))
}

pub(super) fn sections(
    page: &mut GeneralTab,
    g: &GeneralCx,
    window: &mut Window,
    cx: &mut Context<GeneralTab>,
) -> Vec<PageBlock> {
    [
        chat_section(page, g, window, cx),
        status_indicators_section(page, g, window, cx),
        browser_section(page, g, window, cx),
        dev_servers_section(page, g, window, cx),
        editor_section(page, g, cx),
        file_opening_section(page, g, window, cx),
    ]
    .into_iter()
    .flatten()
    .collect()
}
