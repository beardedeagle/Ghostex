use crate::data::*;
use crate::json::opt;
use crate::rows::{row, section, Section};

/// CDXC:Extensions 2026-08-30:
/// The built-in view switches are owned by Settings → Extensions and are
/// searchable from that page's own definitions. General no longer claims
/// them, so a query like "kanban" lands on Extensions instead of matching a
/// General page that has no such section to scroll to.
pub(crate) fn file_opening() -> Section {
    section(
        "fileOpening",
        "File opening",
        vec![
            row("documentFileOpenTarget", "HTML, Markdown and images", "Choose where HTML, Markdown and picture files open from Quick Access, agent chat and the terminal: Files, VS Code, or an editor app installed on this computer.").option_list(crate::file_opening::file_open_target_options()),
            row("otherFileOpenTarget", "Other files", "Choose where every other file (code and text) opens from Quick Access, agent chat and the terminal: Files, VS Code, or an editor app installed on this computer.").option_list(crate::file_opening::file_open_target_options()),
            row("videoFileOpenTarget", "Videos", "Choose whether videos linked in agent chat or the terminal play in Files or open in the system app. Formats Files cannot play, such as .mp4 and .mov, always open in the system app.").options(MEDIA_FILE_OPEN_TARGET_OPTIONS),
            row("fileChangesOpenTarget", "Open file changes in", "Choose where Review all on an agent's file changes and Session details' Changes card open: the Changes view or the VS Code view. If the chosen view is turned off, the other one opens.").options(FILE_CHANGES_OPEN_TARGET_OPTIONS),
            row("audioFileOpenTarget", "Audio", "Choose whether audio files linked in agent chat or the terminal play in Files or open in the system app. Formats Files cannot play, such as .m4a, always open in the system app.").options(MEDIA_FILE_OPEN_TARGET_OPTIONS),
        ],
    )
}

pub(crate) fn browser() -> Section {
    section(
        "browser",
        "Browser",
        vec![
            row("webLinkOpenTarget", "Open links in", "Open web links from terminal output (Command-click), session chat, and detected dev servers in the project Browser view or the system default browser.").options(WEB_LINK_OPEN_TARGET_OPTIONS),
            row("sessionLinkOpenTarget", "Open session links in", "Open a session's pull requests, issues and Linear tickets (the chips after its title and its links list) in the project Browser view or the system default browser. The phone follows this setting too.").options(SESSION_LINK_OPEN_TARGET_OPTIONS),
        ],
    )
}

pub(crate) fn editor() -> Section {
    section(
        "editor",
        "Editor",
        vec![
            row("codeServerLinkVscodeUserConfig", "Use VS Code settings", "Use the VS Code settings from the local VS Code install."),
            row("codeServerUseVscodeInsidersUserConfig", "Use VS Code Insiders settings", "Use the VS Code Insiders user settings directory."),
            row("showUntrackedProjectDiffWhenNoTrackedChanges", "Show untracked lines without tracked changes", "When tracked git diff is +0 -0, show untracked line counts in project headers (Starship-style prompts ignore untracked lines)."),
        ],
    )
}

pub(crate) fn terminal_dev_servers() -> Section {
    section(
        "terminalDevServers",
        "Dev Servers",
        vec![
            row(
                "terminalDevServerDetectionEnabled",
                "Detect running servers in terminals",
                "Detect localhost dev server URLs from terminal output.",
            ),
            row(
                "terminalDevServerIgnoredPortRules",
                "Ignored ports",
                "Hide detected servers on specific ports or inclusive port ranges.",
            )
            .options(&[opt("9229", "9229"), opt("24678-24680", "24678-24680")]),
        ],
    )
}
