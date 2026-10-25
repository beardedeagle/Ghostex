//! One file's diff, as the File diff dialog draws it.
use serde_json::Value;

/// `ChangedFilesTreeStat`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct GitFileStat {
    pub(crate) additions: u64,
    pub(crate) deletions: u64,
}

fn text(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_string)
}

fn count(value: &Value, key: &str) -> u64 {
    value
        .get(key)
        .and_then(|value| {
            value
                .as_u64()
                .or_else(|| value.as_f64().map(|n| n.max(0.0) as u64))
        })
        .unwrap_or(0)
}

/// `SidebarGitFileDiffDraft`: one file's patch, delivered as the `gitFileDiff` open message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GitFileDiffDraft {
    pub(crate) file_path: String,
    pub(crate) patch: String,
    pub(crate) additions: Option<u64>,
    pub(crate) deletions: Option<u64>,
}

impl GitFileDiffDraft {
    pub(crate) fn from_json(draft: &Value) -> Option<Self> {
        let optional = |key: &str| {
            draft
                .get(key)
                .filter(|v| v.is_number())
                .map(|_| count(draft, key))
        };
        Some(Self {
            file_path: text(draft, "filePath")?,
            patch: text(draft, "patch").unwrap_or_default(),
            additions: optional("additions"),
            deletions: optional("deletions"),
        })
    }
}
