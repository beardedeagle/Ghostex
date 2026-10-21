// C1 wave-1 deferred split: apps/desktop/src/app/helpers/remote.rs (~8.2k
// lines) further divided into responsibility-scoped submodules (pure move,
// no logic changes). This file holds remote-machine settings parsing and
// remote sidebar command/repo-clone param builders. See
// docs/2026-08-22/repo-restructure/SPLITS.md C1.

use crate::app::helpers::*;
use crate::*;

pub(crate) fn gpui_remote_machine_config_from_settings(
    object: &serde_json::Map<String, serde_json::Value>,
    remote_machine_id: &str,
) -> Option<GpuiRemoteMachineConfig> {
    /*
    CDXC:RemoteMachines 2026-06-24-14:34:
    Remote reconnect must source SSH host/user/port/identity/password marker from the normalized shared Settings snapshot, not from the React command. This keeps the modal command bounded to an id/approval flag and prevents injected hostnames, paths, passwords, tokens, or shell text from crossing the app-modal bridge.
    */
    let machines = object.get("remoteMachines")?.as_array()?;
    let machine = machines.iter().find_map(|machine| {
        (gpui_remote_machine_id_from_value(machine).as_deref() == Some(remote_machine_id))
            .then_some(machine.as_object())
            .flatten()
    })?;
    let transport =
        if machine.get("transport").and_then(serde_json::Value::as_str) == Some("easyConnect") {
            GpuiRemoteMachineTransport::EasyConnect
        } else {
            GpuiRemoteMachineTransport::Ssh
        };
    let easy_connect_address = match transport {
        GpuiRemoteMachineTransport::EasyConnect => Some(gpui_remote_machine_string_field(
            machine,
            "easyConnectAddress",
        )?),
        GpuiRemoteMachineTransport::Ssh => None,
    };
    let easy_connect_ssh_port = gpui_remote_machine_ssh_port(machine.get("sshPort"))
        .unwrap_or(GPUI_EASY_CONNECT_DEFAULT_SSH_PORT);
    /*
    CDXC:RemotePairing 2026-09-03:
    An Easy Connect machine's SSH endpoint is the loopback forwarder this app
    runs for it, so the host and port come from the forwarder registry rather
    than from Settings. While no forwarder is running the host stays empty;
    `gpui_remote_ssh_dial_refusal` turns that into an explicit error in every
    SSH builder, because ssh itself would resolve an empty host to this
    computer's own sshd.
    */
    let (ssh_host, ssh_port) = match transport {
        GpuiRemoteMachineTransport::Ssh => (
            gpui_remote_machine_string_field(machine, "sshHost")?,
            gpui_remote_machine_ssh_port(machine.get("sshPort")),
        ),
        GpuiRemoteMachineTransport::EasyConnect => {
            match gpui_easy_connect_forward_port(remote_machine_id) {
                Some(port) => (GPUI_EASY_CONNECT_FORWARD_HOST.to_string(), Some(port)),
                None => (String::new(), None),
            }
        }
    };
    let wsl_distribution = gpui_remote_machine_string_field(machine, "wslDistribution");
    if wsl_distribution
        .as_deref()
        .is_some_and(|distribution| !gpui_remote_wsl_distribution_is_valid(distribution))
    {
        return None;
    }
    Some(GpuiRemoteMachineConfig {
        remote_machine_id: remote_machine_id.to_string(),
        transport,
        easy_connect_address,
        easy_connect_ssh_port,
        ssh_host,
        ssh_identity_file: gpui_remote_machine_string_field(machine, "sshIdentityFile")
            .map(|path| gpui_expand_remote_identity_file(path.as_str())),
        ssh_port,
        ssh_user: gpui_remote_machine_string_field(machine, "sshUser"),
        has_saved_password: machine
            .get("sshPasswordSaved")
            .and_then(serde_json::Value::as_bool)
            == Some(true),
        wsl_distribution,
        disabled: machine.get("disabled").and_then(serde_json::Value::as_bool) == Some(true),
    })
}

pub(crate) fn gpui_disabled_remote_machine_ids(
    object: &serde_json::Map<String, serde_json::Value>,
) -> Vec<String> {
    object
        .get("remoteMachines")
        .and_then(serde_json::Value::as_array)
        .map(|machines| {
            machines
                .iter()
                .filter_map(|machine| {
                    let machine_object = machine.as_object()?;
                    if machine_object
                        .get("disabled")
                        .and_then(serde_json::Value::as_bool)
                        != Some(true)
                    {
                        return None;
                    }
                    gpui_remote_machine_id_from_value(machine)
                })
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn gpui_remote_wsl_distribution_is_valid(distribution: &str) -> bool {
    let distribution = distribution.trim();
    !distribution.is_empty()
        && distribution.chars().count() <= 120
        && !distribution.starts_with('-')
        && distribution.chars().all(|character| {
            character.is_ascii_alphanumeric()
                || matches!(character, '.' | '_' | '+' | '-' | '(' | ')' | ' ')
        })
}

pub(crate) fn gpui_remote_machine_name_from_settings(remote_machine_id: &str) -> Option<String> {
    shared_settings::shared_sidebar_settings_snapshot()
        .object()
        .get("remoteMachines")
        .and_then(serde_json::Value::as_array)
        .and_then(|machines| {
            machines.iter().find_map(|machine| {
                let object = machine.as_object()?;
                (gpui_remote_machine_id_from_value(machine).as_deref() == Some(remote_machine_id))
                    .then(|| gpui_remote_machine_string_field(object, "name"))
                    .flatten()
            })
        })
}

pub(crate) fn gpui_remote_machine_string_field(
    machine: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Option<String> {
    machine
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

pub(crate) fn gpui_remote_request_id_from_command(
    command: &serde_json::Map<String, serde_json::Value>,
) -> Option<String> {
    command
        .get("requestId")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .filter(|value| value.chars().count() <= GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS)
        .filter(|value| !value.contains('\0'))
        .map(str::to_string)
}

pub(crate) fn gpui_remote_gxserver_presentation_client_id(remote_machine_id: &str) -> String {
    format!("{GPUI_SIDEBAR_GXSERVER_CLIENT_ID}:{remote_machine_id}")
}

pub(crate) fn gpui_remote_path_like_string_from_command(
    command: &serde_json::Map<String, serde_json::Value>,
    key: &str,
    allow_empty: bool,
) -> Option<String> {
    command
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| allow_empty || !value.is_empty())
        .filter(|value| value.chars().count() <= GPUI_PROJECT_CONTRACT_PATH_MAX_CHARS)
        .filter(|value| !value.contains('\0'))
        .map(str::to_string)
}

/*
CDXC:AddProject 2026-07-30:
Every server round trip the shared add-project dialog performs crosses this one
bridge command. The renderer sends a bounded operation name plus the fields that
operation is allowed to carry; Rust owns the endpoint map, the timeouts, and the
local-vs-remote routing, so the CEF modal host never learns a host, a tunnel
port, or a token — it only ever names a machine by its bounded id.
*/
