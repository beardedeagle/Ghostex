//! Remote page preview: `createRemoteStoryRpc` of packages/core-ui/settings-modal.stories.tsx (deleted 2026-10-01) as a
//! scripted gxserver (Easy Connect running with a pairing code and two paired devices, Tailscale
//! detected, SSH access on), answering from in-memory state so toggles, Turn on SSH access and
//! Remove behave like the real daemon.
//!
//! States: `remote` (the Remote story), `remote-easy-connect` (Easy Connect expanded),
//! `remote-computer` (its Connect a computer tab), `remote-qr` (the Show QR dialog),
//! `remote-paired-confirm` (a paired device's Remove confirmation), `remote-ssh-off`
//! (RemoteSshAccessOff, Easy Connect expanded), `remote-ssh-popover` (SSH off with the Windows
//! instructions open), `remote-ec-off` (RemoteEasyConnectOff, expanded), `remote-tailscale`,
//! `remote-tailscale-manual` (Tailscale expanded, typed values shown), `remote-tailscale-qr`
//! (Tailscale's code in the Show QR dialog), `remote-add-machine`,
//! `remote-add-ec` (Add a machine in Easy Connect code mode with a code pasted),
//! `remote-edit-machine` (the saved machine's dialog through `initialRemoteMachineId`),
//! `remote-advanced` (Advanced open with the raw JSON), `remote-no-server` (no gxserver
//! connection), and the deep links `remote-deeplink-ec` / `remote-deeplink-tailscale`
//! (`initialRemoteSection`: the card opens and scrolls to the top). The page reads the other
//! states from `preview_state` (settings_modal/tabs/remote/mod.rs `apply_preview_state`).
use serde_json::{Map, Value, json};
use std::cell::RefCell;

const ADDRESS: &str = "tc1q8v3k2m9x7p4r6t8w1y5z2a4c6e8g0j3l5n7q9s1u3w5y7a9c1e3g5i7k9m1o3q5s7u9w1y3";

struct RemoteFixture {
    easy_connect: Value,
    access: Value,
    devices: Vec<Value>,
}

thread_local! {
    static FIXTURE: RefCell<Option<RemoteFixture>> = const { RefCell::new(None) };
}

fn iso_now_minus_hours(hours: i64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default()
        - hours * 3600;
    let days = now.div_euclid(86_400);
    let seconds = now.rem_euclid(86_400);
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.000Z",
        seconds / 3600,
        (seconds % 3600) / 60,
        seconds % 60
    )
}

fn fixture_for(state: &str) -> RemoteFixture {
    let easy_connect_enabled = state != "remote-ec-off";
    let ssh_enabled = !matches!(state, "remote-ssh-off" | "remote-ssh-popover");
    RemoteFixture {
        easy_connect: json!({
            "allowedClientKeys": [],
            "binaryFound": true,
            "binaryPath": "/Applications/Ghostex.app/Contents/Resources/bin/tailcat",
            "binaryVersion": "0.4.2",
            "enabled": easy_connect_enabled,
            "lastError": null,
            "ports": [22, 58744],
            "running": easy_connect_enabled,
            "token": if easy_connect_enabled { json!(ADDRESS) } else { Value::Null },
        }),
        access: json!({
            "computerName": "Mohamad's Laptop",
            "platform": "macos",
            "ssh": { "checkedAt": iso_now_minus_hours(0), "detail": null, "enabled": ssh_enabled, "port": 22 },
            "tailscale": {
                "account": "madda@github",
                "installed": true,
                "ip": "100.77.81.4",
                "magicDnsName": "laptop.tail1a2b.ts.net",
                "running": true,
                "sshEnabled": false,
            },
            "username": "madda",
        }),
        devices: vec![
            json!({
                "id": "dev-1",
                "lastSeenAt": iso_now_minus_hours(0),
                "name": "Pixel 9 Pro",
                "pairedAt": iso_now_minus_hours(0),
                "platform": "android",
                "sshKeyFingerprint": "SHA256:aaaa",
            }),
            json!({
                "id": "dev-2",
                "lastSeenAt": iso_now_minus_hours(26),
                "name": "Studio",
                "pairedAt": "2026-09-01T10:00:00.000Z",
                "platform": "macos",
                "sshKeyFingerprint": "SHA256:bbbb",
            }),
        ],
    }
}

fn is_remote_state(state: &str) -> bool {
    state == "remote" || state.starts_with("remote-")
}

pub(super) fn story_settings(state: &str, settings: &mut Map<String, Value>) {
    if !is_remote_state(state) {
        return;
    }
    FIXTURE.with(|fixture| *fixture.borrow_mut() = Some(fixture_for(state)));
    // Only the Remote story saves a machine; the SSH-off and Easy-Connect-off stories have none.
    if !matches!(
        state,
        "remote-ssh-off" | "remote-ssh-popover" | "remote-ec-off" | "remote-empty"
    ) {
        settings.insert(
            "remoteMachines".into(),
            json!([
                { "id": "story-remote", "name": "Remote", "sshHost": "100.105.82.19", "sshPasswordSaved": true, "sshUser": "madda" }
            ]),
        );
    }
}

pub(super) fn open_message(state: &str) -> Option<Value> {
    if !is_remote_state(state) {
        return None;
    }
    let mut message = json!({ "initialTab": "remote" });
    // Only the deep-link states go through the open message; the others are page states the
    // page reads from `preview_state`, so they match the stories' clicks (no scroll).
    match state {
        "remote-deeplink-ec" => message["initialRemoteSection"] = json!("easyConnect"),
        "remote-deeplink-tailscale" => message["initialRemoteSection"] = json!("tailscale"),
        // The story's `story-remote` id is not a `remote-*` id, so the normalized list renumbers it.
        "remote-edit-machine" => message["initialRemoteMachineId"] = json!("remote-1"),
        _ => {}
    }
    Some(message)
}

pub(super) fn gxserver_rpc_available(state: &str) -> bool {
    state != "remote-no-server"
}

fn encode_code(prefix: &str, code: &Value) -> String {
    use base64::Engine as _;
    let json = serde_json::to_string(code).unwrap_or_default();
    format!(
        "{prefix}{}",
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(json)
    )
}

pub(super) fn rpc(path: &str, params: &Value) -> Result<Value, String> {
    FIXTURE.with(|fixture| {
        let mut fixture = fixture.borrow_mut();
        let fixture = fixture.get_or_insert_with(|| fixture_for("remote"));
        match path {
            "/api/tailcatStatus" => Ok(json!({ "status": fixture.easy_connect })),
            "/api/updateTailcatState" => {
                match params.get("kind").and_then(Value::as_str) {
                    Some("setEnabled") => {
                        let enabled = params.get("enabled").and_then(Value::as_bool) == Some(true);
                        fixture.easy_connect["enabled"] = json!(enabled);
                        fixture.easy_connect["running"] = json!(enabled);
                        fixture.easy_connect["token"] =
                            if enabled { json!(ADDRESS) } else { Value::Null };
                    }
                    Some("setPorts") => {
                        if let Some(ports) = params.get("ports").filter(|ports| ports.is_array()) {
                            fixture.easy_connect["ports"] = ports.clone();
                        }
                    }
                    Some("setAllowedClientKeys") => {
                        if let Some(keys) = params
                            .get("allowedClientKeys")
                            .filter(|keys| keys.is_array())
                        {
                            fixture.easy_connect["allowedClientKeys"] = keys.clone();
                        }
                    }
                    _ => {}
                }
                Ok(json!({ "status": fixture.easy_connect }))
            }
            "/api/remoteAccessStatus" => Ok(fixture.access.clone()),
            "/api/enableSshAccess" => Ok(json!({
                "message": "The admin prompt was cancelled.",
                "outcome": "cancelled",
                "ssh": fixture.access["ssh"],
            })),
            "/api/installTailcat" => Ok(json!({ "status": fixture.easy_connect })),
            "/api/remotePairingCode" => {
                let computer = fixture.access["computerName"].clone();
                let user = fixture.access["username"].clone();
                let tailscale_code = json!({
                    "v": 1,
                    "name": computer,
                    "host": "laptop.tail1a2b.ts.net",
                    "ip": "100.77.81.4",
                    "port": 22,
                    "user": user,
                });
                let easy_connect_code = json!({
                    "v": 1,
                    "address": ADDRESS,
                    "name": computer,
                    "user": user,
                    "port": 58744,
                    "sshPort": 22,
                    "secret": "story-secret",
                });
                let mut result = json!({
                    "tailscale": {
                        "code": tailscale_code,
                        "payload": encode_code("ghostex-ts1:", &tailscale_code),
                    },
                });
                if fixture.easy_connect["enabled"] == json!(true) {
                    result["easyConnect"] = json!({
                        "code": easy_connect_code,
                        "payload": encode_code("ghostex-ec1:", &easy_connect_code),
                    });
                }
                Ok(result)
            }
            "/api/pairedDevices" => Ok(json!({ "devices": fixture.devices })),
            "/api/removePairedDevice" => {
                let id = params
                    .get("deviceId")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                fixture
                    .devices
                    .retain(|device| device.get("id").and_then(Value::as_str) != Some(id));
                Ok(json!({ "devices": fixture.devices }))
            }
            other => Err(format!("Story gxserver has no handler for {other}.")),
        }
    })
}

/// `probeRemoteGxserverInstall`: the story machine runs gxserver 0.9.3.
pub(super) fn answers(message: &Value) -> Vec<Value> {
    if message.get("type").and_then(Value::as_str) != Some("probeRemoteGxserverInstall") {
        return Vec::new();
    }
    let Some(id) = message.get("remoteMachineId").and_then(Value::as_str) else {
        return Vec::new();
    };
    vec![json!({
        "installed": true,
        "remoteMachineId": id,
        "type": "remoteGxserverInstallState",
        "version": "0.9.3",
    })]
}
