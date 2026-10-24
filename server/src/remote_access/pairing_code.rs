use std::sync::{Arc, Mutex};

use anyhow::Result;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::{DateTime, Duration, Utc};
use rand::RngCore;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::repository::RemotePairingRepository;

/*
CDXC:RemotePairing 2026-09-03:
Mirror of `packages/shared/ghostex-remote-pairing.ts`: each code is
`<prefix><base64url(compact JSON)>` with no padding and no whitespace. Field
names are the wire names; keep them identical to the TS interfaces.
*/
pub const EASY_CONNECT_CODE_PREFIX: &str = "ghostex-ec1:";
pub const TAILSCALE_CODE_PREFIX: &str = "ghostex-ts1:";

/*
CDXC:RemotePairing 2026-10-24 DECISION:
User: "even tho i have ghostex installed on my phone it says there aren't any
apps that can open this link when i open it (this is on android when i use
default camera, pls fix, i want same on ios also if possible)". A bare
`ghostex-ec1:…` code reads to a system camera as a link with the unknown
scheme `ghostex-ec1`, which no app claims. The QR now carries the code as the
fragment of an https link on ghostex.dev, which the phone app claims as an
Android App Link and an iOS Universal Link, so the system camera opens the
app straight into pairing. Without the app, the link opens ghostex.dev/pair,
a page that says to install Ghostex. The code stays after `#` because a
browser never sends the fragment to the server, so the one-time secret
cannot reach ghostex.dev's logs; its lifetime and one-time use are unchanged.
The app's scanner and the desktop's code field read the link and the bare
code alike.
SEE-ALSO: apps/mobile/app/app.json (intentFilters, associatedDomains),
maddada-com/public/.well-known/ (assetlinks.json, apple-app-site-association).
*/
pub const PAIRING_LINK_PREFIX: &str = "https://ghostex.dev/pair#";

pub const REMOTE_PAIRING_SECRET_TTL: Duration = Duration::minutes(15);

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EasyConnectCode {
    pub v: u8,
    pub address: String,
    pub name: String,
    pub user: String,
    pub port: u16,
    pub ssh_port: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secret: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TailscaleCode {
    pub v: u8,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ip: Option<String>,
    pub port: u16,
    pub user: String,
}

pub fn encode_easy_connect_code(code: &EasyConnectCode) -> String {
    format!("{EASY_CONNECT_CODE_PREFIX}{}", encode_base64url_json(code))
}

pub fn encode_tailscale_code(code: &TailscaleCode) -> String {
    format!("{TAILSCALE_CODE_PREFIX}{}", encode_base64url_json(code))
}

/// The QR payload: an encoded pairing code wrapped in the ghostex.dev link that
/// opens the phone app.
pub fn pairing_link(code: &str) -> String {
    format!("{PAIRING_LINK_PREFIX}{code}")
}

fn encode_base64url_json<T: Serialize>(value: &T) -> String {
    let json = serde_json::to_vec(value).expect("pairing code serializes");
    URL_SAFE_NO_PAD.encode(json)
}

/// A live pairing secret: the plaintext goes into the QR, only the hash is
/// stored. The process keeps the plaintext so consecutive status polls show
/// the same QR until the secret expires or a device consumes it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemotePairingSecret {
    pub secret: String,
    pub secret_hash: String,
    pub expires_at: DateTime<Utc>,
}

impl RemotePairingSecret {
    pub fn expires_at_iso(&self) -> String {
        self.expires_at
            .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
    }
}

#[derive(Clone, Default)]
pub struct RemotePairingRuntime {
    live_secret: Arc<Mutex<Option<RemotePairingSecret>>>,
}

impl RemotePairingRuntime {
    pub fn new() -> Self {
        Self::default()
    }

    /*
    CDXC:RemotePairing 2026-09-03:
    Reuse the live secret while the stored hash still matches it and it has
    not expired; anything else (first poll, expiry, a consumed row after a
    device paired, a database restored from elsewhere) mints a fresh one, so
    the QR silently rotates without the UI having to know why.
    */
    pub fn ensure_secret(&self, db: &Connection) -> Result<RemotePairingSecret> {
        let repository = RemotePairingRepository::new(db);
        let now = Utc::now();
        let stored = repository.read_secret()?;
        let mut live = self.live_secret.lock().expect("remote pairing secret lock");
        if let (Some(live_secret), Some(stored)) = (live.as_ref(), stored.as_ref()) {
            if live_secret.secret_hash == stored.secret_hash && live_secret.expires_at > now {
                return Ok(live_secret.clone());
            }
        }
        let fresh = mint_remote_pairing_secret(now + REMOTE_PAIRING_SECRET_TTL);
        repository.replace_secret(&fresh.secret_hash, &fresh.expires_at_iso())?;
        *live = Some(fresh.clone());
        Ok(fresh)
    }

    /// Drops the in-memory plaintext, e.g. after `/api/pairDevice` consumed
    /// the stored row, so the next poll mints a new code.
    pub fn forget_secret(&self) {
        *self.live_secret.lock().expect("remote pairing secret lock") = None;
    }
}

fn mint_remote_pairing_secret(expires_at: DateTime<Utc>) -> RemotePairingSecret {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    let secret = URL_SAFE_NO_PAD.encode(bytes);
    RemotePairingSecret {
        secret_hash: hash_remote_pairing_secret(&secret),
        secret,
        expires_at,
    }
}

pub fn hash_remote_pairing_secret(secret: &str) -> String {
    format!("{:x}", Sha256::digest(secret.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn easy_connect_code() -> EasyConnectCode {
        EasyConnectCode {
            v: 1,
            address: "tc1qexampleaddress".to_string(),
            name: "Studio".to_string(),
            user: "madda".to_string(),
            port: 58744,
            ssh_port: 22,
            secret: Some("s3cr3t-pairing-secret".to_string()),
            expires_at: Some("2026-10-24T07:15:00.000Z".to_string()),
        }
    }

    #[test]
    fn easy_connect_code_keeps_its_bare_format() {
        let encoded = encode_easy_connect_code(&easy_connect_code());
        let body = encoded.strip_prefix("ghostex-ec1:").expect("bare prefix");
        let json: serde_json::Value =
            serde_json::from_slice(&URL_SAFE_NO_PAD.decode(body).expect("base64url")).unwrap();
        assert_eq!(json["secret"], "s3cr3t-pairing-secret");
        assert_eq!(json["sshPort"], 22);
    }

    #[test]
    fn pairing_link_wraps_the_code_in_the_fragment() {
        let code = encode_easy_connect_code(&easy_connect_code());
        let link = pairing_link(&code);
        assert_eq!(link, format!("https://ghostex.dev/pair#{code}"));
        let (before_fragment, fragment) = link.split_once('#').expect("fragment");
        assert_eq!(before_fragment, "https://ghostex.dev/pair");
        assert_eq!(fragment, code);
        // Nothing a web server sees (scheme, host, path, query) carries the code.
        assert!(!before_fragment.contains("ghostex-ec1"));
        assert!(!before_fragment.contains('?'));
    }

    #[test]
    fn tailscale_link_uses_the_same_wrapper() {
        let code = encode_tailscale_code(&TailscaleCode {
            v: 1,
            name: "Studio".to_string(),
            host: Some("studio.tail1234.ts.net".to_string()),
            ip: None,
            port: 22,
            user: "madda".to_string(),
        });
        assert!(code.starts_with("ghostex-ts1:"));
        assert_eq!(pairing_link(&code), format!("https://ghostex.dev/pair#{code}"));
    }
}
