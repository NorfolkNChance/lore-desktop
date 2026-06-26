//! `FfiLoreClient` — binds directly to liblore in-process (Phase D).
//!
//! This is the structural fix for the CLI wrapper's limitations: no text
//! parsing, no subprocess/server hangs, structured data, and real in-process
//! event streaming (liblore delivers results through an event callback —
//! `lore_event_callback_config_t`). Built only under `--features liblore` with
//! the vendored shared library (`scripts/fetch-liblore.sh`).
//!
//! Status: the binding, linking, and a first real call (`lore_version`) are
//! proven end-to-end. The event-callback-driven operations (status, locks,
//! branches, history, …) are mapped incrementally; until then they return a
//! clear "not yet implemented" error and the default build keeps using the CLI
//! wrapper. The `LoreClient` seam means filling these in touches nothing else.

#![allow(non_upper_case_globals, non_camel_case_types, non_snake_case, dead_code)]

/// Raw bindgen output for lore.h.
mod sys {
    include!(concat!(env!("OUT_DIR"), "/liblore_bindings.rs"));
}

use super::{ClientMode, LoreClient, LoreError, LoreResult};
use crate::models::*;
use async_trait::async_trait;
use std::ffi::CStr;
use std::path::PathBuf;

pub struct FfiLoreClient {
    repository: PathBuf,
}

impl FfiLoreClient {
    pub fn new(repository: PathBuf) -> Self {
        Self { repository }
    }

    /// The liblore library version (proves the FFI binding + linking work).
    pub fn library_version() -> String {
        // SAFETY: `lore_version` returns a pointer to a static, NUL-terminated
        // C string owned by the library; we only borrow it.
        unsafe {
            let p = sys::lore_version();
            if p.is_null() {
                "unknown".to_string()
            } else {
                CStr::from_ptr(p).to_string_lossy().into_owned()
            }
        }
    }
}

fn pending(op: &str) -> LoreError {
    LoreError::Cli(format!(
        "liblore FFI: `{op}` not yet implemented — build without `--features liblore` for the full CLI-backed client"
    ))
}

#[async_trait]
impl LoreClient for FfiLoreClient {
    fn mode(&self) -> ClientMode {
        ClientMode::Ffi
    }

    async fn version(&self) -> LoreResult<String> {
        Ok(format!("liblore {}", Self::library_version()))
    }

    async fn status(&self) -> LoreResult<WorkspaceStatus> {
        let _ = &self.repository;
        Err(pending("status"))
    }

    async fn query_locks(&self) -> LoreResult<Vec<Lock>> {
        Err(pending("query_locks"))
    }

    async fn lock_status(&self, _path: &str) -> LoreResult<LockState> {
        Err(pending("lock_status"))
    }

    async fn acquire_lock(&self, _path: &str, _reason: Option<String>) -> LoreResult<Lock> {
        Err(pending("acquire_lock"))
    }

    async fn release_lock(&self, _path: &str) -> LoreResult<()> {
        Err(pending("release_lock"))
    }

    async fn stage(&self, _paths: &[String]) -> LoreResult<()> {
        Err(pending("stage"))
    }

    async fn unstage(&self, _paths: &[String]) -> LoreResult<()> {
        Err(pending("unstage"))
    }

    async fn commit(&self, _message: &str) -> LoreResult<String> {
        Err(pending("commit"))
    }

    async fn list_branches(&self) -> LoreResult<Vec<Branch>> {
        Err(pending("list_branches"))
    }

    async fn switch_branch(&self, _name: &str) -> LoreResult<()> {
        Err(pending("switch_branch"))
    }

    async fn create_branch(&self, _name: &str) -> LoreResult<()> {
        Err(pending("create_branch"))
    }

    async fn history(&self, _limit: Option<u32>) -> LoreResult<Vec<Revision>> {
        Err(pending("history"))
    }

    async fn sync(&self, _revision: Option<String>) -> LoreResult<()> {
        Err(pending("sync"))
    }

    async fn push(&self, _branch: Option<String>) -> LoreResult<()> {
        Err(pending("push"))
    }

    async fn current_identity(&self) -> LoreResult<Identity> {
        Err(pending("current_identity"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn library_version_is_nonempty() {
        // Proves the binding links and a real liblore call returns data.
        let v = FfiLoreClient::library_version();
        assert!(!v.is_empty());
        assert_ne!(v, "unknown");
    }
}
