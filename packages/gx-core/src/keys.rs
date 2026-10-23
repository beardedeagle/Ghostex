//! Typed identity of machines, projects, and sessions.
//!
//! The old runtime passed string-encoded ids everywhere and parsed them at every use. The core
//! uses typed keys; the string forms exist only at the persistence edge (the shell layout, the
//! focus state file, client storage) and at the boundary to code that still speaks the old forms.

use serde::{Deserialize, Serialize};

/// Which daemon owns a project or session.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MachineId {
    /// The daemon on this computer.
    Local,
    /// A saved remote machine, by its settings id (for example `remote-ab12`).
    Remote(String),
}

impl MachineId {
    pub fn is_local(&self) -> bool {
        matches!(self, Self::Local)
    }

    pub fn remote_id(&self) -> Option<&str> {
        match self {
            Self::Local => None,
            Self::Remote(machine_id) => Some(machine_id.as_str()),
        }
    }
}

/// A project on one machine. Project ids are unique per daemon only.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectKey {
    pub machine: MachineId,
    pub project_id: String,
}

/// A session on one machine. Session ids are unique only together with the project id.
///
/// The raw `project_id` and `session_id` are what that machine's daemon accepts; a scoped string
/// form must never be sent to a daemon.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionKey {
    pub machine: MachineId,
    pub project_id: String,
    pub session_id: String,
}

const COMBINED_SESSION_PREFIX: &str = "combined-session:";
const COMBINED_PROJECT_GROUP_PREFIX: &str = "combined-project:";
const REMOTE_PREFIX: &str = "remote:";
const REMOTE_PROJECT_INFIX: &str = ":project:";
const REMOTE_SESSION_INFIX: &str = ":session:";
const REMOTE_GROUP_INFIX: &str = ":group:";
const WORKSPACE_SUBGROUP_PREFIX: &str = "gpui-wsg:";

impl ProjectKey {
    pub fn local(project_id: impl Into<String>) -> Self {
        Self {
            machine: MachineId::Local,
            project_id: project_id.into(),
        }
    }

    pub fn remote(machine_id: impl Into<String>, project_id: impl Into<String>) -> Self {
        Self {
            machine: MachineId::Remote(machine_id.into()),
            project_id: project_id.into(),
        }
    }

    /// The workspace project id the desktop shell persists: the raw id for a local project,
    /// `remote:<machine>:project:<project>` for a remote one.
    pub fn to_workspace_project_id(&self) -> String {
        match &self.machine {
            MachineId::Local => self.project_id.clone(),
            MachineId::Remote(machine_id) => {
                format!(
                    "{REMOTE_PREFIX}{machine_id}{REMOTE_PROJECT_INFIX}{}",
                    self.project_id
                )
            }
        }
    }

    /// Inverse of [`Self::to_workspace_project_id`]. Any string that is not a remote project id is
    /// a local raw project id; an empty string is no project.
    pub fn parse_workspace_project_id(value: &str) -> Option<Self> {
        if value.is_empty() {
            return None;
        }
        if let Some(rest) = value.strip_prefix(REMOTE_PREFIX) {
            // `^remote:([^:]+):project:(.+)$`
            let (machine_id, project_id) = rest.split_once(':')?;
            let project_id = project_id.strip_prefix(&REMOTE_PROJECT_INFIX[1..])?;
            if machine_id.is_empty() || project_id.is_empty() {
                return None;
            }
            return Some(Self::remote(machine_id, project_id));
        }
        Some(Self::local(value))
    }

    /// The sidebar group id of this project: `combined-project:<encoded project>` for a local
    /// project, `remote:<machine>:group:<project>` for a remote one.
    pub fn to_sidebar_group_id(&self) -> String {
        match &self.machine {
            MachineId::Local => format!(
                "{COMBINED_PROJECT_GROUP_PREFIX}{}",
                encode_uri_component(&self.project_id)
            ),
            MachineId::Remote(machine_id) => {
                format!(
                    "{REMOTE_PREFIX}{machine_id}{REMOTE_GROUP_INFIX}{}",
                    self.project_id
                )
            }
        }
    }

    /// Inverse of [`Self::to_sidebar_group_id`].
    pub fn parse_sidebar_group_id(value: &str) -> Option<Self> {
        if let Some(encoded) = value.strip_prefix(COMBINED_PROJECT_GROUP_PREFIX) {
            let project_id = decode_uri_component(encoded)?;
            return (!project_id.is_empty()).then(|| Self::local(project_id));
        }
        // `^remote:([^:]+):group:(.+)$`
        let rest = value.strip_prefix(REMOTE_PREFIX)?;
        let (machine_id, project_id) = rest.split_once(':')?;
        let project_id = project_id.strip_prefix(&REMOTE_GROUP_INFIX[1..])?;
        if machine_id.is_empty() || project_id.is_empty() {
            return None;
        }
        Some(Self::remote(machine_id, project_id))
    }
}

impl SessionKey {
    pub fn local(project_id: impl Into<String>, session_id: impl Into<String>) -> Self {
        Self {
            machine: MachineId::Local,
            project_id: project_id.into(),
            session_id: session_id.into(),
        }
    }

    pub fn remote(
        machine_id: impl Into<String>,
        project_id: impl Into<String>,
        session_id: impl Into<String>,
    ) -> Self {
        Self {
            machine: MachineId::Remote(machine_id.into()),
            project_id: project_id.into(),
            session_id: session_id.into(),
        }
    }

    pub fn project_key(&self) -> ProjectKey {
        ProjectKey {
            machine: self.machine.clone(),
            project_id: self.project_id.clone(),
        }
    }

    /// The sidebar session id: `combined-session:<encoded project>:<encoded session>` for a local
    /// session (both parts URI-component encoded), `remote:<machine>:session:<project>:<session>`
    /// for a remote one. This is the form client storage keeps for "last session of a project".
    pub fn to_sidebar_session_id(&self) -> String {
        match &self.machine {
            MachineId::Local => format!(
                "{COMBINED_SESSION_PREFIX}{}:{}",
                encode_uri_component(&self.project_id),
                encode_uri_component(&self.session_id)
            ),
            MachineId::Remote(_) => self.to_focus_state_session_id(),
        }
    }

    /// Inverse of [`Self::to_sidebar_session_id`].
    pub fn parse_sidebar_session_id(value: &str) -> Option<Self> {
        if let Some(payload) = value.strip_prefix(COMBINED_SESSION_PREFIX) {
            let (project_id, session_id) = payload.split_once(':')?;
            let project_id = decode_uri_component(project_id)?;
            let session_id = decode_uri_component(session_id)?;
            if project_id.is_empty() || session_id.is_empty() {
                return None;
            }
            return Some(Self::local(project_id, session_id));
        }
        Self::parse_remote_scoped_session_id(value)
    }

    /// The id the focus state file and the old focus bridge use: the RAW session id for a local
    /// session (no project part), the remote-scoped id for a remote one.
    ///
    /// The local form cannot be parsed back without the store, because it does not name the
    /// project; resolve it with `PresentationStore::resolve_focus_state_session_id`.
    pub fn to_focus_state_session_id(&self) -> String {
        match &self.machine {
            MachineId::Local => self.session_id.clone(),
            MachineId::Remote(machine_id) => format!(
                "{REMOTE_PREFIX}{machine_id}{REMOTE_SESSION_INFIX}{}:{}",
                self.project_id, self.session_id
            ),
        }
    }

    /// Parses `remote:<machine>:session:<project>:<session>`
    /// (`^remote:([^:]+):session:([^:]+):(.+)$`). Returns `None` for any other string, including
    /// every local id.
    pub fn parse_remote_scoped_session_id(value: &str) -> Option<Self> {
        let rest = value.strip_prefix(REMOTE_PREFIX)?;
        let (machine_id, rest) = rest.split_once(':')?;
        let rest = rest.strip_prefix(&REMOTE_SESSION_INFIX[1..])?;
        let (project_id, session_id) = rest.split_once(':')?;
        if machine_id.is_empty() || project_id.is_empty() || session_id.is_empty() {
            return None;
        }
        Some(Self::remote(machine_id, project_id, session_id))
    }
}

/// Sidebar id of a user-made session group: `gpui-wsg:<encoded workspace project id>:<group id>`.
pub fn encode_workspace_subgroup_id(project: &ProjectKey, group_id: &str) -> String {
    format!(
        "{WORKSPACE_SUBGROUP_PREFIX}{}:{group_id}",
        encode_uri_component(&project.to_workspace_project_id())
    )
}

/// Inverse of [`encode_workspace_subgroup_id`]: the project and the user group id.
pub fn parse_workspace_subgroup_id(value: &str) -> Option<(ProjectKey, String)> {
    let rest = value.strip_prefix(WORKSPACE_SUBGROUP_PREFIX)?;
    let (encoded_project, group_id) = rest.split_once(':')?;
    if encoded_project.is_empty() || group_id.is_empty() {
        return None;
    }
    let project = ProjectKey::parse_workspace_project_id(&decode_uri_component(encoded_project)?)?;
    Some((project, group_id.to_string()))
}

/// JavaScript `encodeURIComponent`: every byte is percent-encoded except ASCII letters, digits,
/// and `- _ . ~ ! * ' ( )`.
pub fn encode_uri_component(value: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z'
            | b'a'..=b'z'
            | b'0'..=b'9'
            | b'-'
            | b'_'
            | b'.'
            | b'~'
            | b'!'
            | b'*'
            | b'\''
            | b'('
            | b')' => encoded.push(char::from(byte)),
            _ => {
                encoded.push('%');
                encoded.push(char::from(HEX[usize::from(byte >> 4)]));
                encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
            }
        }
    }
    encoded
}

/// JavaScript `decodeURIComponent`. `None` where JavaScript throws: a `%` that is not followed by
/// two hex digits, or decoded bytes that are not UTF-8.
pub fn decode_uri_component(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while let Some(&byte) = bytes.get(index) {
        if byte == b'%' {
            let high = hex_value(*bytes.get(index + 1)?)?;
            let low = hex_value(*bytes.get(index + 2)?)?;
            decoded.push((high << 4) | low);
            index += 3;
        } else {
            decoded.push(byte);
            index += 1;
        }
    }
    String::from_utf8(decoded).ok()
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}
