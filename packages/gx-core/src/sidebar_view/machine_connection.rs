//! What a remote machine's connection state looks like in the sidebar: whether its tab spins or
//! turns red, and the notice its list draws while it is not connected.
//!
//! CDXC:RemoteMachines 2026-10-25 DECISION:
//! User: a remote machine that cannot connect must never show an empty tab with no message. Every state that is not connected, connecting or plain `disconnected` is a failure, and the machine's list says why with Reconnect and Remote Settings. Supersedes the 2026-10-02 notice that sat in the list as a red title over two stacked buttons ("these messages at the top are so ugly"). The user's picks for the look: the failure is a card "docked at top, not bottom" of the list with the reason and labelled Reconnect and Remote Settings buttons, plus a "tinted icon (cloud icon becomes redish when fails)"; "tint just the icon not the whole tab"; connecting is the tab's "spinner and the one line connecting to macbook... pill floating near the top", naming the machine. The rows never move for it: "i dont want the projects and sessions to shift down while we connect or have errors etc. these sessions should stay in same spot (floating above" — the card and the pill float over the top of the list with no inset.
//!
//! SEE-ALSO: apps/desktop/src/app/helpers/remote/types.rs (`GpuiRemoteGxserverConnectState`, the
//! wire words, and the terminal overlay that reads the same copy),
//! apps/desktop/src/app/native_sidebar/machines.rs and machine_notice.rs (the renderers).

use super::inputs::{MachineTabInput, MACHINE_STATE_CONNECTED};
use super::view::MachineNotice;

/// A connect attempt is running for the machine.
pub fn machine_state_is_busy(state: &str) -> bool {
    matches!(
        state,
        "connecting" | "installing" | "downloadingRemoteServerPackage"
    )
}

/// The machine's last connect attempt failed (`sshFailed`, `authFailed`, `tunnelFailed`, …).
pub fn machine_state_is_failure(state: &str) -> bool {
    !machine_state_is_busy(state) && !matches!(state, MACHINE_STATE_CONNECTED | "disconnected")
}

/// The title and the default explanation of a connection state, shared by the sidebar notice and
/// the terminal overlay of a remote session so the two never disagree about one machine.
pub fn machine_state_copy(state: &str) -> (&'static str, Option<&'static str>) {
    match state {
        "installing" => ("Installing gxserver…", None),
        "downloadingRemoteServerPackage" => ("Downloading server package…", None),
        "disconnected" => ("Not connected", None),
        "installApprovalRequired" => (
            "Remote setup needed",
            Some("Approve the gxserver install for this machine."),
        ),
        "installFailed" => (
            "Remote setup failed",
            Some("Reconnect the machine to try again."),
        ),
        "authFailed" => (
            "Cannot sign in to machine",
            Some("The machine rejected the saved SSH username, password or key."),
        ),
        "sshFailed" => ("Cannot reach machine", Some("SSH connection failed.")),
        "tunnelFailed" => ("Cannot reach machine", Some("SSH tunnel failed.")),
        "keychainFailed" => (
            "Cannot reach machine",
            Some("Saved credentials are unavailable."),
        ),
        "tokenUnavailable" => (
            "Cannot reach machine",
            Some("The remote gxserver token is unavailable."),
        ),
        "presentationSubscribeFailed" | "presentationStreamFailed" => (
            "Reconnecting to machine…",
            Some("The remote session stream dropped."),
        ),
        "unsupported" | "unsupportedRemotePlatform" => (
            "Machine unsupported",
            Some("This remote platform cannot host gxserver."),
        ),
        "invalid" => (
            "Machine unavailable",
            Some("The saved machine settings are incomplete."),
        ),
        "failed" => (
            "Cannot reach machine",
            Some("Reconnect the machine to try again."),
        ),
        _ => ("Connecting to machine…", None),
    }
}

/// The notice of the selected machine's list: none for this computer or a connected machine.
/// A failure explains itself with the host's sanitized reason when it has one.
pub(crate) fn machine_notice(machine: Option<&MachineTabInput>) -> Option<MachineNotice> {
    let machine = machine.filter(|machine| !machine.is_local() && !machine.is_connected())?;
    let (title, detail) = machine_state_copy(&machine.state);
    let failed = machine_state_is_failure(&machine.state);
    let busy = machine_state_is_busy(&machine.state);
    // The floating pill names the machine; the terminal overlay keeps the shared generic copy.
    let title = if machine.state == "connecting" {
        format!("Connecting to {}…", machine.label)
    } else {
        title.to_string()
    };
    Some(MachineNotice {
        machine_id: machine.machine_id.clone(),
        title,
        detail: if failed {
            machine.message.clone().or(detail.map(str::to_string))
        } else {
            detail.map(str::to_string)
        },
        busy,
        failed,
    })
}
