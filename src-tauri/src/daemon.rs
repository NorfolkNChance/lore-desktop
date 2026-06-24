//! Cross-platform live-update daemon.
//!
//! `lore service run` ("IPC not supported on this OS" on macOS in 0.8.3) is not
//! a reliable basis for live updates, so instead of shelling out to it we run
//! our own watcher: a filesystem watch on the working tree (debounced) emits
//! `statusChanged`, and a periodic tick emits `lockChanged` to re-query
//! server-side locks. Works identically on macOS, Windows, and Linux.
//!
//! `ServiceState::Running` means the watcher is active. When liblore lands
//! (Phase D), its in-process event subscription replaces the periodic poll.

use crate::lore::LoreConfig;
use crate::models::{LoreEvent, LoreEventTag, LoreLogLevel, ServiceState};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tokio::sync::Mutex;

const LORE_EVENT_CHANNEL: &str = "lore://event";
const DEBOUNCE: Duration = Duration::from_millis(500);
const LOCK_POLL: Duration = Duration::from_secs(30);

pub struct DaemonController {
    inner: Mutex<DaemonInner>,
    config: LoreConfig,
}

#[derive(Default)]
struct DaemonInner {
    state: ServiceState,
    /// Kept alive while running; dropping it stops the OS watch.
    watcher: Option<RecommendedWatcher>,
    /// Signals the background task to stop.
    stop: Option<tokio::sync::watch::Sender<bool>>,
}

impl Default for ServiceState {
    fn default() -> Self {
        ServiceState::Stopped
    }
}

impl DaemonController {
    pub fn new(config: LoreConfig) -> Self {
        Self { inner: Mutex::new(DaemonInner::default()), config }
    }

    pub async fn state(&self) -> ServiceState {
        self.inner.lock().await.state
    }

    /// Start watching the configured repository. No-op (stays `Stopped`) when no
    /// repository is configured (the mock backend needs no watcher).
    pub async fn start(&self, app: &AppHandle) {
        let repo = match &self.config.repository {
            Some(r) if r.exists() => r.clone(),
            _ => {
                log::info!("daemon: no repository configured; watcher idle");
                return;
            }
        };
        {
            let inner = self.inner.lock().await;
            if matches!(inner.state, ServiceState::Running | ServiceState::Starting) {
                return;
            }
        }
        set_state(self, app, ServiceState::Starting).await;

        // Bridge notify's (sync, own-thread) callback to async via an unbounded
        // channel. Ignore churn inside `.lore/` to avoid feedback loops.
        let (fs_tx, mut fs_rx) = tokio::sync::mpsc::unbounded_channel::<()>();
        let watcher_result = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            if let Ok(event) = res {
                let internal = event
                    .paths
                    .iter()
                    .all(|p| p.components().any(|c| c.as_os_str() == ".lore"));
                if !internal {
                    let _ = fs_tx.send(());
                }
            }
        });

        let mut watcher = match watcher_result {
            Ok(w) => w,
            Err(e) => {
                log::error!("daemon: failed to create watcher: {e}");
                set_state(self, app, ServiceState::Error).await;
                return;
            }
        };
        if let Err(e) = watcher.watch(&repo, RecursiveMode::Recursive) {
            log::error!("daemon: failed to watch {}: {e}", repo.display());
            set_state(self, app, ServiceState::Error).await;
            return;
        }

        let (stop_tx, mut stop_rx) = tokio::sync::watch::channel(false);
        let app_bg = app.clone();
        tokio::spawn(async move {
            let mut poll = tokio::time::interval(LOCK_POLL);
            poll.tick().await; // consume the immediate first tick
            loop {
                tokio::select! {
                    _ = stop_rx.changed() => break,
                    Some(()) = fs_rx.recv() => {
                        // Debounce: let a burst of edits settle, then refresh once.
                        tokio::time::sleep(DEBOUNCE).await;
                        while fs_rx.try_recv().is_ok() {}
                        emit(&app_bg, LoreEventTag::StatusChanged, serde_json::json!({ "source": "watcher" }));
                    }
                    _ = poll.tick() => {
                        emit(&app_bg, LoreEventTag::LockChanged, serde_json::json!({ "source": "poll" }));
                    }
                }
            }
        });

        let mut inner = self.inner.lock().await;
        inner.watcher = Some(watcher);
        inner.stop = Some(stop_tx);
        inner.state = ServiceState::Running;
        drop(inner);
        emit_state(app, ServiceState::Running);
        log::info!("daemon: watching {}", repo.display());
    }

    /// Stop the watcher. Safe to call when already stopped.
    pub async fn stop(&self, app: &AppHandle) {
        let mut inner = self.inner.lock().await;
        if let Some(stop) = inner.stop.take() {
            let _ = stop.send(true);
        }
        inner.watcher = None; // dropping unregisters the OS watch
        inner.state = ServiceState::Stopped;
        drop(inner);
        emit_state(app, ServiceState::Stopped);
        log::info!("daemon: stopped");
    }
}

async fn set_state(ctl: &DaemonController, app: &AppHandle, next: ServiceState) {
    ctl.inner.lock().await.state = next;
    emit_state(app, next);
}

fn emit_state(app: &AppHandle, state: ServiceState) {
    emit(app, LoreEventTag::ServiceStateChanged, serde_json::json!({ "state": state }));
}

fn emit(app: &AppHandle, tag: LoreEventTag, payload: serde_json::Value) {
    let event = LoreEvent {
        tag,
        timestamp: chrono::Utc::now().to_rfc3339(),
        level: LoreLogLevel::Info,
        payload: Some(payload),
    };
    let _ = app.emit(LORE_EVENT_CHANNEL, event);
}
