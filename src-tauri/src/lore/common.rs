//! Status-assembly logic shared by every backend.
//!
//! The CLI and FFI clients must produce identical `WorkspaceStatus` semantics —
//! same asset classification, same lock-merge behaviour, same me-vs-other
//! attribution. Keeping that logic here (instead of per-backend copies) is what
//! enforces it; the backends only differ in how they *fetch* raw data.

use crate::models::*;
use std::collections::HashMap;
use std::time::Duration;

/// Bound on the server-side lock query folded into `status()`. A slow or
/// unreachable remote degrades to `LockState::Unknown` rather than stalling
/// the UI.
pub const LOCK_MERGE_TIMEOUT: Duration = Duration::from_secs(6);

/// Classify an asset by extension for the binary-first UI.
pub fn asset_kind_for(path: &str) -> AssetKind {
    let ext = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "umap" => AssetKind::Umap,
        "uasset" => AssetKind::Uasset,
        "png" | "tga" | "jpg" | "jpeg" | "exr" | "dds" => AssetKind::Texture,
        "wav" | "ogg" | "mp3" => AssetKind::Audio,
        "txt" | "ini" | "json" | "xml" | "cpp" | "h" | "rs" | "ts" | "md" | "toml" => {
            AssetKind::Text
        }
        _ => AssetKind::Binary,
    }
}

/// True if a lock owner string refers to the authenticated identity.
///
/// Attribution rules (the single source of truth for me-vs-other):
/// - an empty or `<unknown>` owner is treated as "me" — the server couldn't
///   name the holder, and flagging our own lock as someone else's would block
///   the release button;
/// - without a login we can't tell, so fall back to "me" (single-user
///   assumption);
/// - otherwise substring-match against the user id and display name — CLI
///   output may embed the id inside a longer owner string.
pub fn owner_is_me(owner: &str, identity: &Identity) -> bool {
    let o = owner.trim();
    if o.is_empty() || o == "<unknown>" {
        return true;
    }
    if !identity.authenticated {
        return true;
    }
    (!identity.user_id.is_empty() && o.contains(&identity.user_id))
        || (!identity.name.is_empty() && o.contains(&identity.name))
}

/// Build a `Lock` from a lock-query result (path + raw owner string),
/// attributing it via [`owner_is_me`].
pub fn lock_entry(path: String, owner: String, identity: &Identity) -> Lock {
    let state = if owner_is_me(&owner, identity) {
        LockState::LockedByMe
    } else {
        LockState::LockedByOther
    };
    Lock {
        path,
        state,
        owner: Some(Author { name: owner, email: String::new() }),
        instance_id: None,
        acquired_at: None,
        reason: None,
    }
}

/// Fold a lock-query result into a freshly assembled status.
///
/// `Some(locks)`: each entry gets its lock (or `Unlocked` when absent) and
/// `locks_available` is set. `None` (query failed / timed out): entries are
/// `Unknown` — *not* `Unlocked`, so the UI never implies a file is free to
/// edit when we simply don't know. Counts are recomputed either way.
pub fn merge_locks(status: &mut WorkspaceStatus, locks: Option<Vec<Lock>>) {
    match locks {
        Some(locks) => {
            let by_path: HashMap<&str, &Lock> =
                locks.iter().map(|l| (l.path.as_str(), l)).collect();
            for e in &mut status.entries {
                match by_path.get(e.path.as_str()) {
                    Some(l) => {
                        e.lock_state = l.state;
                        e.lock = Some((*l).clone());
                    }
                    None => {
                        e.lock_state = LockState::Unlocked;
                        e.lock = None;
                    }
                }
            }
            status.locks_available = true;
        }
        None => {
            for e in &mut status.entries {
                e.lock_state = LockState::Unknown;
                e.lock = None;
            }
            status.locks_available = false;
        }
    }
    status.counts = StatusCounts::from_entries(&status.entries);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity() -> Identity {
        Identity {
            user_id: "u-123".into(),
            name: "James Burns".into(),
            authenticated: true,
        }
    }

    fn entry(path: &str, staged: bool, change: FileChange) -> FileEntry {
        let kind = asset_kind_for(path);
        FileEntry {
            path: path.into(),
            file_id: String::new(),
            change,
            staged,
            dirty: true,
            is_binary: kind.is_binary(),
            asset_kind: kind,
            size_bytes: 0,
            fragment_count: 0,
            lock_state: LockState::Unknown,
            lock: None,
        }
    }

    fn status_with(entries: Vec<FileEntry>) -> WorkspaceStatus {
        WorkspaceStatus {
            workspace_id: String::new(),
            branch: Branch {
                id: String::new(),
                name: "main".into(),
                latest_revision: String::new(),
                protected: false,
            },
            head_revision: Revision {
                id: String::new(),
                parents: vec![],
                message: String::new(),
                author: Author { name: String::new(), email: String::new() },
                timestamp: String::new(),
                tree_root: FragmentAddress { hash: String::new(), context: String::new() },
                is_merge: false,
            },
            counts: StatusCounts::from_entries(&entries),
            entries,
            locks_available: false,
        }
    }

    #[test]
    fn classifies_assets_by_extension() {
        assert!(matches!(asset_kind_for("Content/Maps/A.umap"), AssetKind::Umap));
        assert!(matches!(asset_kind_for("Content/B.uasset"), AssetKind::Uasset));
        assert!(matches!(asset_kind_for("T/albedo.PNG"), AssetKind::Texture)); // case-insensitive
        assert!(matches!(asset_kind_for("Audio/boom.wav"), AssetKind::Audio));
        assert!(matches!(asset_kind_for("Source/main.rs"), AssetKind::Text));
        assert!(matches!(asset_kind_for("Binaries/game.pak"), AssetKind::Binary));
        assert!(matches!(asset_kind_for("LICENSE"), AssetKind::Binary)); // no extension
    }

    #[test]
    fn unknown_or_empty_owner_is_me() {
        assert!(owner_is_me("<unknown>", &identity()));
        assert!(owner_is_me("", &identity()));
        assert!(owner_is_me("   ", &identity()));
    }

    #[test]
    fn without_login_every_owner_is_me() {
        assert!(owner_is_me("Dana Reyes", &Identity::unauthenticated()));
    }

    #[test]
    fn authenticated_owner_matches_by_id_or_name_substring() {
        let id = identity();
        assert!(owner_is_me("u-123", &id));
        assert!(owner_is_me("James Burns", &id));
        assert!(owner_is_me("James Burns (u-123)", &id)); // id embedded in a longer string
        assert!(!owner_is_me("Dana Reyes", &id));
    }

    #[test]
    fn empty_identity_fields_never_match() {
        let id = Identity { user_id: String::new(), name: String::new(), authenticated: true };
        assert!(!owner_is_me("Dana Reyes", &id));
    }

    #[test]
    fn lock_entry_attributes_state() {
        let id = identity();
        assert_eq!(lock_entry("a".into(), "u-123".into(), &id).state, LockState::LockedByMe);
        assert_eq!(
            lock_entry("a".into(), "Dana Reyes".into(), &id).state,
            LockState::LockedByOther
        );
    }

    #[test]
    fn merge_with_locks_marks_matched_and_unlocked() {
        let mut status = status_with(vec![
            entry("A.umap", true, FileChange::Modified),
            entry("B.uasset", false, FileChange::Added),
        ]);
        let locks = vec![lock_entry("A.umap".into(), "Dana Reyes".into(), &identity())];
        merge_locks(&mut status, Some(locks));

        assert!(status.locks_available);
        assert_eq!(status.entries[0].lock_state, LockState::LockedByOther);
        assert!(status.entries[0].lock.is_some());
        assert_eq!(status.entries[1].lock_state, LockState::Unlocked);
        assert!(status.entries[1].lock.is_none());
        assert_eq!(status.counts.locked_by_other, 1);
        assert_eq!(status.counts.locked_by_me, 0);
        assert_eq!(status.counts.staged, 1);
        assert_eq!(status.counts.modified, 1);
    }

    #[test]
    fn merge_without_locks_leaves_state_unknown() {
        let mut status = status_with(vec![entry("A.umap", false, FileChange::Modified)]);
        merge_locks(&mut status, None);

        assert!(!status.locks_available);
        assert_eq!(status.entries[0].lock_state, LockState::Unknown);
        assert_eq!(status.counts.locked_by_me, 0);
        assert_eq!(status.counts.locked_by_other, 0);
    }
}
