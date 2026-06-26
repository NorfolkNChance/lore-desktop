//! App-managed state shared across IPC commands.
//!
//! The active backend (client + lock manager + mode + repository) is swappable
//! at runtime so the user can open or clone a different repository without
//! restarting — see `set_repository`. Reads take an `Arc<ActiveBackend>`
//! snapshot (cheap clone, no lock held across `.await`); a switch atomically
//! replaces the snapshot.

use crate::lock_manager::LockManager;
use crate::lore::{self, ClientMode, LoreClient, LoreConfig};
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

/// One configured backend bound to a single repository.
pub struct ActiveBackend {
    pub client: Arc<dyn LoreClient>,
    pub locks: LockManager,
    pub mode: ClientMode,
    pub repository: Option<PathBuf>,
}

pub struct AppState {
    backend: RwLock<Arc<ActiveBackend>>,
    /// Retained so a runtime repository switch can rebuild the client.
    binary: Option<PathBuf>,
}

impl AppState {
    pub fn from_config(config: &LoreConfig) -> Self {
        AppState {
            backend: RwLock::new(Arc::new(Self::build(config))),
            binary: config.binary.clone(),
        }
    }

    fn build(config: &LoreConfig) -> ActiveBackend {
        let client: Arc<dyn LoreClient> = Arc::from(lore::build_client(config));
        let mode = client.mode();
        ActiveBackend {
            locks: LockManager::new(client.clone()),
            client,
            mode,
            repository: config.repository.clone(),
        }
    }

    /// A cheap snapshot of the active backend. The returned `Arc` keeps the
    /// backend alive across awaits even if a switch happens mid-call.
    pub fn backend(&self) -> Arc<ActiveBackend> {
        self.backend.read().unwrap().clone()
    }

    /// Point the app at a different repository, rebuilding the client. Returns
    /// the new backend so the caller can read its mode/repository.
    pub fn set_repository(&self, repository: PathBuf) -> Arc<ActiveBackend> {
        let config = LoreConfig {
            binary: self.binary.clone(),
            repository: Some(repository),
        };
        let backend = Arc::new(Self::build(&config));
        *self.backend.write().unwrap() = backend.clone();
        backend
    }

    pub fn binary(&self) -> Option<PathBuf> {
        self.binary.clone()
    }
}
