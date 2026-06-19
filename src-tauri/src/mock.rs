//! Static mock data for Phase 1.
//!
//! Returns realistic, internally-consistent fixtures matching the data
//! contracts so the React UI can be built against a live IPC surface before
//! liblore is wired in (Phase 2). The scenario deliberately exercises the
//! binary-first cases the UI must emphasize: large `.uasset`/`.umap` files,
//! locks held by "me" vs. "others", and mixed staged/dirty state.

use crate::models::*;

fn me() -> Author {
    Author {
        name: "James Burns".into(),
        email: "norfolknchance@gmail.com".into(),
    }
}

fn teammate() -> Author {
    Author {
        name: "Dana Reyes".into(),
        email: "dana@studio.example".into(),
    }
}

fn addr(hash: &str, context: &str) -> FragmentAddress {
    FragmentAddress {
        hash: hash.into(),
        context: context.into(),
    }
}

pub fn workspace() -> Workspace {
    Workspace {
        id: "018f9b2a-7c41-7e10-9a3d-0a1b2c3d4e5f".into(),
        name: "FortniteSandbox".into(),
        path: "/Users/jamesburns/projects/fortnite-sandbox".into(),
        shared_store_path: "/Users/jamesburns/.lore/stores/fortnite-sandbox".into(),
        current_branch_id: "018f9b2a-1000-7000-8000-000000000001".into(),
        current_revision: "9f1c4a2b8e7d6c5f0a1b2c3d4e5f60718293a4b5".into(),
        view: vec!["Content/Maps/**".into(), "Content/Characters/**".into()],
        dirty: true,
        staged_file_count: 2,
    }
}

pub fn branch() -> Branch {
    Branch {
        id: "018f9b2a-1000-7000-8000-000000000001".into(),
        name: "main".into(),
        latest_revision: "9f1c4a2b8e7d6c5f0a1b2c3d4e5f60718293a4b5".into(),
        protected: true,
    }
}

pub fn revisions() -> Vec<Revision> {
    vec![
        Revision {
            id: "9f1c4a2b8e7d6c5f0a1b2c3d4e5f60718293a4b5".into(),
            parents: vec!["7a2b3c4d5e6f708192a3b4c5d6e7f8091a2b3c4d".into()],
            message: "Rebake lighting for Volcano_Island, import hero rig".into(),
            author: me(),
            timestamp: "2026-06-19T14:48:00Z".into(),
            tree_root: addr(
                "1122334455667788990011223344556677889900aabbccddeeff001122334455",
                "00000000000000000000000000000001",
            ),
            is_merge: false,
        },
        Revision {
            id: "7a2b3c4d5e6f708192a3b4c5d6e7f8091a2b3c4d".into(),
            parents: vec![
                "5102030405060708090a0b0c0d0e0f1011121314".into(),
                "33445566778899aabbccddeeff0011223344556".into(),
            ],
            message: "Merge feature/foliage-lod into main".into(),
            author: teammate(),
            timestamp: "2026-06-18T22:05:00Z".into(),
            tree_root: addr(
                "aabbccddeeff00112233445566778899aabbccddeeff001122334455667788990",
                "00000000000000000000000000000002",
            ),
            is_merge: true,
        },
        Revision {
            id: "5102030405060708090a0b0c0d0e0f1011121314".into(),
            parents: vec![],
            message: "Initial import of project skeleton".into(),
            author: me(),
            timestamp: "2026-06-15T09:00:00Z".into(),
            tree_root: addr(
                "00ff00ff00ff00ff00ff00ff00ff00ff00ff00ff00ff00ff00ff00ff00ff00ff",
                "00000000000000000000000000000003",
            ),
            is_merge: false,
        },
    ]
}

fn lock_me(path: &str, reason: &str) -> Lock {
    Lock {
        path: path.into(),
        state: LockState::LockedByMe,
        owner: Some(me()),
        instance_id: Some("018f9b2a-7c41-7e10-9a3d-0a1b2c3d4e5f".into()),
        acquired_at: Some("2026-06-19T14:30:00Z".into()),
        reason: Some(reason.into()),
    }
}

fn lock_other(path: &str, reason: &str) -> Lock {
    Lock {
        path: path.into(),
        state: LockState::LockedByOther,
        owner: Some(teammate()),
        instance_id: Some("018f9b2a-7c41-7e10-9a3d-ffffffffffff".into()),
        acquired_at: Some("2026-06-19T11:12:00Z".into()),
        reason: Some(reason.into()),
    }
}

pub fn file_entries() -> Vec<FileEntry> {
    vec![
        FileEntry {
            path: "Content/Maps/Volcano_Island.umap".into(),
            file_id: "0a1b2c3d4e5f60718293a4b5c6d7e8f9".into(),
            change: FileChange::Modified,
            staged: true,
            dirty: true,
            is_binary: true,
            asset_kind: AssetKind::Umap,
            size_bytes: 2_415_919_104, // ~2.25 GiB
            fragment_count: 1180,
            lock_state: LockState::LockedByMe,
            lock: Some(lock_me(
                "Content/Maps/Volcano_Island.umap",
                "Rebaking lighting",
            )),
        },
        FileEntry {
            path: "Content/Characters/Hero/Hero_Skeleton.uasset".into(),
            file_id: "1b2c3d4e5f60718293a4b5c6d7e8f9a0".into(),
            change: FileChange::Added,
            staged: true,
            dirty: true,
            is_binary: true,
            asset_kind: AssetKind::Uasset,
            size_bytes: 184_549_376, // ~176 MiB
            fragment_count: 92,
            lock_state: LockState::LockedByMe,
            lock: Some(lock_me(
                "Content/Characters/Hero/Hero_Skeleton.uasset",
                "Importing new rig",
            )),
        },
        FileEntry {
            path: "Content/Characters/Hero/BP_Hero.uasset".into(),
            file_id: "2c3d4e5f60718293a4b5c6d7e8f9a0b1".into(),
            change: FileChange::Modified,
            staged: false,
            dirty: true,
            is_binary: true,
            asset_kind: AssetKind::Blueprint,
            size_bytes: 6_291_456, // 6 MiB
            fragment_count: 4,
            // Blocked: a teammate holds this lock. The UI must surface this loudly.
            lock_state: LockState::LockedByOther,
            lock: Some(lock_other(
                "Content/Characters/Hero/BP_Hero.uasset",
                "Reworking ability graph",
            )),
        },
        FileEntry {
            path: "Content/Materials/M_Lava.uasset".into(),
            file_id: "3d4e5f60718293a4b5c6d7e8f9a0b1c2".into(),
            change: FileChange::Modified,
            staged: false,
            dirty: true,
            is_binary: true,
            asset_kind: AssetKind::Material,
            size_bytes: 33_554_432, // 32 MiB
            fragment_count: 18,
            // Modified locally but NOT locked — risky for unmergeable content.
            lock_state: LockState::Unlocked,
            lock: None,
        },
        FileEntry {
            path: "Config/DefaultGame.ini".into(),
            file_id: "4e5f60718293a4b5c6d7e8f9a0b1c2d3".into(),
            change: FileChange::Modified,
            staged: false,
            dirty: true,
            is_binary: false,
            asset_kind: AssetKind::Text,
            size_bytes: 8_192,
            fragment_count: 1,
            lock_state: LockState::Unlocked,
            lock: None,
        },
    ]
}

pub fn workspace_status() -> WorkspaceStatus {
    let entries = file_entries();
    let counts = StatusCounts {
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
    };

    WorkspaceStatus {
        workspace_id: workspace().id,
        branch: branch(),
        head_revision: revisions().into_iter().next().unwrap(),
        entries,
        counts,
    }
}

pub fn locks() -> Vec<Lock> {
    file_entries().into_iter().filter_map(|e| e.lock).collect()
}
