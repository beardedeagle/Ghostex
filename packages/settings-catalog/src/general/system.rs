use crate::data::*;
use crate::json::opt;
use crate::rows::{option, row, section, Section};

pub(crate) fn auto_sleep() -> Section {
    section(
        "autoSleep",
        "Auto Sleep",
        vec![
            row(
                "autoSleepCodeEditorIdleMinutes",
                "VS Code Auto Sleep",
                "Choose when inactive VS Code panes sleep, or turn Auto Sleep off.",
            )
            .num_options(AUTO_SLEEP_IDLE_MINUTE_OPTIONS),
            row(
                "autoSleepGitEditorIdleMinutes",
                "Git Auto Sleep",
                "Choose when inactive Git panes sleep, or turn Auto Sleep off.",
            )
            .num_options(AUTO_SLEEP_IDLE_MINUTE_OPTIONS),
            row(
                "autoSleepProjectEditorIdleMinutes",
                "Project Auto Sleep",
                "Choose when inactive Project panes sleep, or turn Auto Sleep off.",
            )
            .num_options(AUTO_SLEEP_IDLE_MINUTE_OPTIONS),
            row(
                "autoSleepBrowserIdleMinutes",
                "Browser Auto Sleep",
                "Choose when inactive browser panes sleep, or turn Auto Sleep off.",
            )
            .num_options(AUTO_SLEEP_IDLE_MINUTE_OPTIONS),
            row(
                "autoSleepAgentIdleMinutes",
                "Agent Auto Sleep",
                "Choose when eligible agent terminals sleep, or turn Auto Sleep off.",
            )
            .num_options(AUTO_SLEEP_IDLE_MINUTE_OPTIONS),
            row(
                "autoSleepRequireAgentResumeCommand",
                "Require resume command",
                "Only auto-sleep agent sessions Ghostex can wake with a resume command.",
            ),
            row(
                "autoSleepFavoriteAgentSessions",
                "Include favorite agents",
                "Allow favorite agent sessions to auto-sleep.",
            ),
        ],
    )
}

pub(crate) fn power() -> Section {
    section(
        "power",
        "Power",
        vec![
            row("hideKeepAwakeTitlebarControl", "Hide Keep Awake", "Hide the Keep Awake entry from the sidebar menu."),
            row("keepAwakeDefaultDurationMinutes", "Default keep-awake duration", "Choose the duration Keep Awake uses by default.").num_options(KEEP_AWAKE_DURATION_OPTIONS),
            row("keepAwakeAllowDisplaySleep", "Allow display sleep", "Keep the computer awake but allow the display to turn off."),
            row("keepAwakePreventLidSleep", "Prevent lid-close sleep", "Optional. When Keep Awake is on, Ghostex can install a small privileged helper once so closing the lid stays awake only for that active keep-awake session."),
            row("keepAwakeActivateOnLaunch", "Activate on launch", "Start preventing sleep when Ghostex launches."),
            row("keepAwakeActivateOnExternalDisplay", "Activate on external display", "Start preventing sleep when an external display is connected."),
            row("keepAwakeWhileWorkingSessions", "Keep awake for working sessions", "Keep the computer awake while sessions are working and for 20 minutes after."),
            row("keepAwakeBatteryThresholdPercent", "Battery threshold", "Stop preventing sleep below this battery level, or turn the rule off.").options(&[opt("Off", "0")]).option_list((0..17).map(|index| {
                let percent = 10 + index * 5;
                option(format!("{percent}%"), percent.to_string())
            })),
            row("keepAwakeDeactivateOnLowPowerMode", "Deactivate in Low Power Mode", "Stop preventing sleep when Low Power Mode is enabled."),
            row("keepAwakeDeactivateOnUserSwitch", "Deactivate on user switch", "Stop preventing sleep when this user session is no longer active."),
        ],
    )
}

pub(crate) fn sleeping_sessions() -> Section {
    section(
        "sleepingSessions",
        "Sleeping Sessions",
        vec![
            row("dimSleepingSessions", "Dim sleeping sessions", "Fade sleeping sessions in the sidebar so they stand apart from awake ones."),
            row("wakeSleepingSessionsOnSelect", "Wake sleeping sessions when selected", "Wake a sleeping session as soon as you select it. Turn off to open it with a Resume button instead, so switching sessions does not wake it by accident."),
        ],
    )
}

pub(crate) fn beta() -> Section {
    section(
        "beta",
        "Experimental",
        vec![
            // CDXC:Settings 2026-06-28-07:41:
            // Settings search should find the advanced experimental gate by label and
            // by the concrete surfaces it enables so the required inventory stays
            // discoverable without tying Agents Hub to this gate.
            row("showBetaFeatures", "Enable Experimental Features", "Show experimental surfaces: OS Integration settings, Browser color scheme, Keep Awake, and Connectors."),
            row("fasterRendering", "Faster rendering", "Redraw only the parts of a window that changed, which uses less CPU while agents stream and you scroll. Experimental: turn it off if part of a window stops updating."),
        ],
    )
}
