//! Daemon lifecycle controller.
//!
//! Owns a child `lore service run` process tied to the app's lifetime: started
//! on setup (when a CLI repository is configured) and stopped gracefully on
//! exit. "Graceful" = ask `lore service stop` first, then hard-kill the child
//! if it's still alive — both paths are cross-platform (tokio `Child::kill`
//! terminates on macOS, Linux, and Windows alike).

use crate::lore::LoreConfig;
use crate::models::{LoreEvent, LoreEventTag, LoreLogLevel, ServiceState};
use tauri::{AppHandle, Emitter};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

const LORE_EVENT_CHANNEL: &str = "lore://event";

pub struct DaemonController {
    inner: Mutex<DaemonInner>,
    config: LoreConfig,
}

struct DaemonInner {
    state: ServiceState,
    child: Option<Child>,
}

impl DaemonController {
    pub fn new(config: LoreConfig) -> Self {
        Self {
            inner: Mutex::new(DaemonInner { state: ServiceState::Stopped, child: None }),
            config,
        }
    }

    pub async fn state(&self) -> ServiceState {
        self.inner.lock().await.state
    }

    async fn set_state(&self, app: &AppHandle, next: ServiceState) {
        self.inner.lock().await.state = next;
        emit_state(app, next);
    }

    /// Spawn `lore service run` for the configured repository. No-op (stays
    /// `Stopped`) when there's no binary/repository — the mock backend needs no
    /// daemon.
    pub async fn start(&self, app: &AppHandle) {
        let (binary, repository) = match (&self.config.binary, &self.config.repository) {
            (Some(b), Some(r)) if r.exists() => (b.clone(), r.clone()),
            _ => {
                log::info!("daemon: no repository configured; staying stopped");
                return;
            }
        };

        {
            let inner = self.inner.lock().await;
            if matches!(inner.state, ServiceState::Running | ServiceState::Starting) {
                return;
            }
        }
        self.set_state(app, ServiceState::Starting).await;

        let spawn = Command::new(&binary)
            .current_dir(&repository)
            .arg("--non-interactive")
            .arg("--repository")
            .arg(&repository)
            .args(["service", "run"])
            .kill_on_drop(true)
            .spawn();

        match spawn {
            Ok(child) => {
                let mut inner = self.inner.lock().await;
                inner.child = Some(child);
                inner.state = ServiceState::Running;
                drop(inner);
                emit_state(app, ServiceState::Running);
                log::info!("daemon: lore service started for {}", repository.display());
            }
            Err(e) => {
                log::error!("daemon: failed to start service: {e}");
                self.set_state(app, ServiceState::Error).await;
            }
        }
    }

    /// Graceful shutdown: ask the service to stop, then kill the child if it
    /// outlives the request. Safe to call when already stopped.
    pub async fn stop(&self, app: &AppHandle) {
        self.set_state(app, ServiceState::Stopping).await;

        if let (Some(binary), Some(repository)) = (&self.config.binary, &self.config.repository) {
            // Best-effort cooperative stop; ignore failure (service may already
            // be gone, or never registered).
            let _ = Command::new(binary)
                .current_dir(repository)
                .arg("--non-interactive")
                .arg("--repository")
                .arg(repository)
                .args(["service", "stop"])
                .output()
                .await;
        }

        let mut inner = self.inner.lock().await;
        if let Some(mut child) = inner.child.take() {
            // If still running, terminate it.
            match child.try_wait() {
                Ok(Some(_)) => {}
                _ => {
                    let _ = child.kill().await;
                }
            }
        }
        inner.state = ServiceState::Stopped;
        drop(inner);
        emit_state(app, ServiceState::Stopped);
        log::info!("daemon: stopped");
    }
}

fn emit_state(app: &AppHandle, state: ServiceState) {
    let event = LoreEvent {
        tag: LoreEventTag::ServiceStateChanged,
        timestamp: chrono::Utc::now().to_rfc3339(),
        level: LoreLogLevel::Info,
        payload: Some(serde_json::json!({ "state": state })),
    };
    let _ = app.emit(LORE_EVENT_CHANNEL, event);
}
