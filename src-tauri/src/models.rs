//! Rust mirror of the TypeScript data contracts in `src/types/lore.ts`.
//!
//! Every struct serializes to camelCase so the JSON crossing the Tauri IPC
//! boundary deserializes directly into the frontend interfaces. These shapes
//! are the seam between the UI and the backend: today the mock layer fills
//! them, in Phase 2 the liblore bindings will.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

// ---------------------------------------------------------------------------
// Content addressing
// ---------------------------------------------------------------------------

/// 48-byte fragment address: 32-byte BLAKE3 hash + 16-byte context tag.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct FragmentAddress {
    /// Hex of the 32-byte BLAKE3 content hash.
    pub hash: String,
    /// Hex of the 16-byte context tag (entity identity; not part of dedup).
    pub context: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum ChunkingStrategy {
    Fastcdc,
    Fixed,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Fragment {
    pub address: FragmentAddress,
    #[ts(type = "number")]
    pub size_bytes: u64,
    /// 16-byte opaque partition id (authorization boundary), hex.
    pub partition: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub chunking: Option<ChunkingStrategy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub offset: Option<u64>,
}

// ---------------------------------------------------------------------------
// History
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Author {
    pub name: String,
    pub email: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Revision {
    pub id: String,
    pub parents: Vec<String>,
    pub message: String,
    pub author: Author,
    pub timestamp: String,
    pub tree_root: FragmentAddress,
    pub is_merge: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Branch {
    pub id: String,
    pub name: String,
    pub latest_revision: String,
    pub protected: bool,
}

// ---------------------------------------------------------------------------
// Workspace (Lore "Instance")
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Workspace {
    pub id: String,
    pub name: String,
    pub path: String,
    pub shared_store_path: String,
    pub current_branch_id: String,
    pub current_revision: String,
    pub view: Vec<String>,
    pub dirty: bool,
    pub staged_file_count: u32,
}

// ---------------------------------------------------------------------------
// Locks
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum LockState {
    Unlocked,
    LockedByMe,
    LockedByOther,
    Stale,
    /// Lock state could not be determined — the server was unreachable or the
    /// lock query timed out. Distinct from `Unlocked` so the UI doesn't imply a
    /// file is free to edit when we simply don't know.
    Unknown,
}

/// The authenticated user, for attributing locks to "me" vs "others".
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Identity {
    pub user_id: String,
    pub name: String,
    /// False when no `lore login` identity is available.
    pub authenticated: bool,
}

impl Identity {
    /// The "not logged in" identity every backend falls back to.
    pub fn unauthenticated() -> Self {
        Identity {
            user_id: String::new(),
            name: String::new(),
            authenticated: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct Lock {
    pub path: String,
    pub state: LockState,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub owner: Option<Author>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub instance_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub acquired_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub reason: Option<String>,
}

// ---------------------------------------------------------------------------
// File / working-tree status
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum FileChange {
    Unchanged,
    Added,
    Modified,
    Deleted,
    Renamed,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum AssetKind {
    Uasset,
    Umap,
    Blueprint,
    Material,
    Texture,
    Audio,
    Binary,
    Text,
}

impl AssetKind {
    /// Unmergeable content — flips the UI from "diff" to "lock" mode.
    pub fn is_binary(self) -> bool {
        !matches!(self, AssetKind::Text)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    pub path: String,
    pub file_id: String,
    pub change: FileChange,
    pub staged: bool,
    pub dirty: bool,
    pub is_binary: bool,
    pub asset_kind: AssetKind,
    #[ts(type = "number")]
    pub size_bytes: u64,
    pub fragment_count: u32,
    pub lock_state: LockState,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub lock: Option<Lock>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct StatusCounts {
    pub staged: u32,
    pub modified: u32,
    pub locked_by_me: u32,
    pub locked_by_other: u32,
}

impl StatusCounts {
    /// Derive the header-badge counts from a set of entries — the one place
    /// this arithmetic lives, so every backend reports identical numbers.
    pub fn from_entries(entries: &[FileEntry]) -> Self {
        StatusCounts {
            staged: entries.iter().filter(|e| e.staged).count() as u32,
            modified: entries
                .iter()
                .filter(|e| matches!(e.change, FileChange::Modified))
                .count() as u32,
            locked_by_me: entries
                .iter()
                .filter(|e| e.lock_state == LockState::LockedByMe)
                .count() as u32,
            locked_by_other: entries
                .iter()
                .filter(|e| e.lock_state == LockState::LockedByOther)
                .count() as u32,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceStatus {
    pub workspace_id: String,
    pub branch: Branch,
    pub head_revision: Revision,
    pub entries: Vec<FileEntry>,
    pub counts: StatusCounts,
    /// True if lock state was successfully resolved from the server. When false,
    /// entries carry `LockState::Unknown` and the UI shows "lock state
    /// unavailable" rather than implying everything is unlocked.
    pub locks_available: bool,
}

// ---------------------------------------------------------------------------
// Daemon events & service lifecycle
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default, TS)]
#[serde(rename_all = "camelCase")]
pub enum ServiceState {
    #[default]
    Stopped,
    Starting,
    Running,
    Stopping,
    Error,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum LoreLogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

/// Progress for multi-gigabyte streaming transfers (Phase 4). Carried by the
/// `transferProgress` event (see `crate::events::LoreEventKind`).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TransferProgress {
    /// Stable id for the operation (e.g. a commit or sync).
    pub operation_id: String,
    pub label: String,
    #[ts(type = "number")]
    pub bytes_done: u64,
    #[ts(type = "number")]
    pub bytes_total: u64,
    pub fragments_done: u32,
    pub fragments_total: u32,
}
