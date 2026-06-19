//! Rust mirror of the TypeScript data contracts in `src/types/lore.ts`.
//!
//! Every struct serializes to camelCase so the JSON crossing the Tauri IPC
//! boundary deserializes directly into the frontend interfaces. These shapes
//! are the seam between the UI and the backend: today the mock layer fills
//! them, in Phase 2 the liblore bindings will.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Content addressing
// ---------------------------------------------------------------------------

/// 48-byte fragment address: 32-byte BLAKE3 hash + 16-byte context tag.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FragmentAddress {
    /// Hex of the 32-byte BLAKE3 content hash.
    pub hash: String,
    /// Hex of the 16-byte context tag (entity identity; not part of dedup).
    pub context: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ChunkingStrategy {
    Fastcdc,
    Fixed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Fragment {
    pub address: FragmentAddress,
    pub size_bytes: u64,
    /// 16-byte opaque partition id (authorization boundary), hex.
    pub partition: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chunking: Option<ChunkingStrategy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<u64>,
}

// ---------------------------------------------------------------------------
// History
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Author {
    pub name: String,
    pub email: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LockState {
    Unlocked,
    LockedByMe,
    LockedByOther,
    Stale,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Lock {
    pub path: String,
    pub state: LockState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<Author>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instance_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub acquired_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

// ---------------------------------------------------------------------------
// File / working-tree status
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FileChange {
    Unchanged,
    Added,
    Modified,
    Deleted,
    Renamed,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    pub path: String,
    pub file_id: String,
    pub change: FileChange,
    pub staged: bool,
    pub dirty: bool,
    pub is_binary: bool,
    pub asset_kind: AssetKind,
    pub size_bytes: u64,
    pub fragment_count: u32,
    pub lock_state: LockState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lock: Option<Lock>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusCounts {
    pub staged: u32,
    pub modified: u32,
    pub locked_by_me: u32,
    pub locked_by_other: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceStatus {
    pub workspace_id: String,
    pub branch: Branch,
    pub head_revision: Revision,
    pub entries: Vec<FileEntry>,
    pub counts: StatusCounts,
}

// ---------------------------------------------------------------------------
// Daemon events & service lifecycle
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ServiceState {
    Stopped,
    Starting,
    Running,
    Stopping,
    Error,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LoreEventTag {
    LockChanged,
    StatusChanged,
    RevisionCommitted,
    BranchSwitched,
    ServiceStateChanged,
    TransferProgress,
    Log,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LoreLogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoreEvent {
    pub tag: LoreEventTag,
    pub timestamp: String,
    pub level: LoreLogLevel,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<serde_json::Value>,
}
