use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const EV_PRESENCE: &str = "nx:presence";
pub const EV_CURSOR: &str = "nx:cursor";
pub const EV_PATCH: &str = "nx:patch";
pub const EV_SNAPSHOT: &str = "nx:snapshot";
pub const EV_SNAP_REQ: &str = "nx:snap_req";
pub const EV_FILE: &str = "nx:file";
pub const EV_LOAD_REQ: &str = "nx:load_req";
pub const EV_SAVE_REQ: &str = "nx:save_req";
pub const EV_WEB_CODE: &str = "code";
pub const EV_WEB_PRESENCE: &str = "presence";

/// Wire envelope exchanged over the LAN WebSocket.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Wire {
    /// "evt" (relay a broadcast), "welcome" (host → joiner full state), "ping"/"pong"
    pub ty: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub files: Option<Vec<WelcomeFile>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub members: Option<Vec<Presence>>,
}

impl Wire {
    pub fn evt(event: &str, payload: Value) -> Self {
        Self {
            ty: "evt".into(),
            event: Some(event.to_string()),
            payload: Some(payload),
            files: None,
            members: None,
        }
    }
    pub fn welcome(files: Vec<WelcomeFile>, members: Vec<Presence>) -> Self {
        Self {
            ty: "welcome".into(),
            event: None,
            payload: None,
            files: Some(files),
            members: Some(members),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct WelcomeFile {
    pub name: String,
    pub lang: String,
    pub code: String,
    #[serde(default)]
    pub rev: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Presence {
    pub id: String,
    pub name: String,
    pub color: String,
    pub ts: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CursorMsg {
    pub id: String,
    pub name: String,
    pub color: String,
    pub file: String,
    pub line: usize,
    pub col: usize,
}

/// A joiner asking the host to persist and git-commit the workspace as `name`
/// (the session owner's repo lives on the host, but any user can save).
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SaveReqMsg {
    pub name: String,
}

/// A replacement of `remove` lines starting at `start` with `lines`.
///
/// `old` holds the removed block's content. For patches applied to the state
/// they were computed against (`base_rev` matches), `old` is redundant; it is
/// required to *rebase* the patch onto newer content when concurrent edits
/// land out of order. `prev`/`next` anchor the insertion point for pure
/// insertions (`old` empty) so a stale insert can be re-located.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct LinePatch {
    pub start: usize,
    pub remove: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub old: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prev: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next: Option<String>,
    pub lines: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PatchMsg {
    pub id: String,
    pub file: String,
    pub lang: String,
    pub base_rev: u64,
    pub patches: Vec<LinePatch>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SnapshotMsg {
    pub id: String,
    pub file: String,
    pub lang: String,
    pub rev: u64,
    pub lines: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SnapReqMsg {
    pub id: String,
    pub file: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FileMsg {
    pub id: String,
    pub name: String,
    pub lang: String,
    pub open: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct WebCodeMsg {
    pub author: String,
    pub file: String,
    pub code: String,
}
