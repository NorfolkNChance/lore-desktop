//! Binary lock manager.
//!
//! The critical module for the engine workflow: a single choke point for
//! acquiring, releasing, and tracking locks on unmergeable assets (`.uasset`,
//! `.umap`). Every mutation emits a `lockChanged` daemon event so all open
//! views refresh instantly. Backed by whichever [`LoreClient`] is active, so it
//! works identically against the real CLI and the mock.

use crate::events::{emit, LoreEventKind};
use crate::lore::{LoreClient, LoreError};
use crate::models::{Lock, LockState};
use std::sync::Arc;
use tauri::AppHandle;

pub struct LockManager {
    client: Arc<dyn LoreClient>,
}

impl LockManager {
    pub fn new(client: Arc<dyn LoreClient>) -> Self {
        Self { client }
    }

    pub async fn list(&self) -> Result<Vec<Lock>, LoreError> {
        self.client.query_locks().await
    }

    pub async fn status(&self, path: &str) -> Result<LockState, LoreError> {
        self.client.lock_status(path).await
    }

    pub async fn acquire(
        &self,
        app: &AppHandle,
        path: &str,
        reason: Option<String>,
    ) -> Result<Lock, LoreError> {
        let lock = self.client.acquire_lock(path, reason).await?;
        emit_lock_changed(app, path, LockState::LockedByMe);
        Ok(lock)
    }

    pub async fn release(&self, app: &AppHandle, path: &str) -> Result<(), LoreError> {
        self.client.release_lock(path).await?;
        emit_lock_changed(app, path, LockState::Unlocked);
        Ok(())
    }
}

fn emit_lock_changed(app: &AppHandle, path: &str, state: LockState) {
    emit(
        app,
        LoreEventKind::LockChanged { path: Some(path.to_string()), state: Some(state) },
    );
}
