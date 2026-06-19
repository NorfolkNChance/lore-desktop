//! App-managed state shared across IPC commands.

use crate::lock_manager::LockManager;
use crate::lore::{self, ClientMode, LoreClient, LoreConfig};
use std::path::PathBuf;
use std::sync::Arc;

pub struct AppState {
    pub client: Arc<dyn LoreClient>,
    pub locks: LockManager,
    pub mode: ClientMode,
    pub repository: Option<PathBuf>,
}

impl AppState {
    pub fn from_config(config: &LoreConfig) -> Self {
        let client: Arc<dyn LoreClient> = Arc::from(lore::build_client(config));
        let mode = client.mode();
        AppState {
            locks: LockManager::new(client.clone()),
            client,
            mode,
            repository: config.repository.clone(),
        }
    }
}
