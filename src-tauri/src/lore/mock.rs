//! `MockLoreClient` — stateful in-memory backend used when `lore` or a
//! repository isn't configured. Unlike the Phase 1 static mock, lock changes
//! persist for the session, so the UI's acquire/release actually stick.

use super::{ClientMode, LoreClient, LoreError, LoreResult};
use crate::models::*;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Mutex;

pub struct MockLoreClient {
    /// path -> current lock disposition. Seeded from the demo fixtures.
    locks: Mutex<HashMap<String, LockState>>,
}

impl MockLoreClient {
    pub fn new() -> Self {
        let seed = crate::mock::file_entries()
            .into_iter()
            .map(|e| (e.path, e.lock_state))
            .collect();
        Self { locks: Mutex::new(seed) }
    }

    fn me() -> Author {
        Author { name: "James Burns".into(), email: "norfolknchance@gmail.com".into() }
    }

    fn teammate() -> Author {
        Author { name: "Dana Reyes".into(), email: "dana@studio.example".into() }
    }

    fn lock_for(path: &str, state: LockState) -> Option<Lock> {
        match state {
            LockState::Unlocked => None,
            LockState::LockedByMe => Some(Lock {
                path: path.into(),
                state,
                owner: Some(Self::me()),
                instance_id: Some("018f9b2a-7c41-7e10-9a3d-0a1b2c3d4e5f".into()),
                acquired_at: Some("2026-06-19T14:30:00Z".into()),
                reason: None,
            }),
            LockState::LockedByOther => Some(Lock {
                path: path.into(),
                state,
                owner: Some(Self::teammate()),
                instance_id: Some("018f9b2a-7c41-7e10-9a3d-ffffffffffff".into()),
                acquired_at: Some("2026-06-19T11:12:00Z".into()),
                reason: Some("Reworking ability graph".into()),
            }),
            LockState::Stale => Some(Lock {
                path: path.into(),
                state,
                owner: None,
                instance_id: None,
                acquired_at: None,
                reason: None,
            }),
        }
    }
}

#[async_trait]
impl LoreClient for MockLoreClient {
    fn mode(&self) -> ClientMode {
        ClientMode::Mock
    }

    async fn version(&self) -> LoreResult<String> {
        Ok("lore (mock backend — no repository configured)".to_string())
    }

    async fn status(&self) -> LoreResult<WorkspaceStatus> {
        let locks = self.locks.lock().unwrap();
        let mut status = crate::mock::workspace_status();
        for entry in &mut status.entries {
            let state = locks.get(&entry.path).copied().unwrap_or(LockState::Unlocked);
            entry.lock_state = state;
            entry.lock = Self::lock_for(&entry.path, state);
        }
        status.counts = StatusCounts {
            staged: status.entries.iter().filter(|e| e.staged).count() as u32,
            modified: status
                .entries
                .iter()
                .filter(|e| matches!(e.change, FileChange::Modified))
                .count() as u32,
            locked_by_me: status
                .entries
                .iter()
                .filter(|e| e.lock_state == LockState::LockedByMe)
                .count() as u32,
            locked_by_other: status
                .entries
                .iter()
                .filter(|e| e.lock_state == LockState::LockedByOther)
                .count() as u32,
        };
        Ok(status)
    }

    async fn query_locks(&self) -> LoreResult<Vec<Lock>> {
        let locks = self.locks.lock().unwrap();
        Ok(locks
            .iter()
            .filter_map(|(path, state)| Self::lock_for(path, *state))
            .collect())
    }

    async fn lock_status(&self, path: &str) -> LoreResult<LockState> {
        let locks = self.locks.lock().unwrap();
        Ok(locks.get(path).copied().unwrap_or(LockState::Unlocked))
    }

    async fn acquire_lock(&self, path: &str, reason: Option<String>) -> LoreResult<Lock> {
        let mut locks = self.locks.lock().unwrap();
        if locks.get(path) == Some(&LockState::LockedByOther) {
            return Err(LoreError::Cli(format!("{path} is already locked by another user")));
        }
        locks.insert(path.to_string(), LockState::LockedByMe);
        let mut lock = Self::lock_for(path, LockState::LockedByMe).unwrap();
        lock.reason = reason;
        Ok(lock)
    }

    async fn release_lock(&self, path: &str) -> LoreResult<()> {
        let mut locks = self.locks.lock().unwrap();
        locks.insert(path.to_string(), LockState::Unlocked);
        Ok(())
    }
}
