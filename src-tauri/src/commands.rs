//! Tauri IPC command handlers.
//!
//! The signatures are unchanged from Phase 1 — only the bodies switched from
//! static mocks to the live [`LoreClient`] (real CLI or stateful mock, chosen
//! at startup). The frontend is unaffected by that swap, which is the whole
//! point of the trait seam.

use crate::daemon::DaemonController;
use crate::lore::ClientMode;
use crate::models::*;
use crate::state::AppState;
use tauri::{AppHandle, State};

// ---------------------------------------------------------------------------
// Backend / service introspection
// ---------------------------------------------------------------------------

/// Which backend is active, so the UI can show a "mock data" banner.
#[tauri::command]
pub fn backend_mode(state: State<'_, AppState>) -> ClientMode {
    state.mode
}

#[tauri::command]
pub async fn lore_version(state: State<'_, AppState>) -> Result<String, String> {
    state.client.version().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn service_state(daemon: State<'_, DaemonController>) -> Result<ServiceState, String> {
    Ok(daemon.state().await)
}

#[tauri::command]
pub async fn start_service(
    app: AppHandle,
    daemon: State<'_, DaemonController>,
) -> Result<(), String> {
    daemon.start(&app).await;
    Ok(())
}

#[tauri::command]
pub async fn stop_service(
    app: AppHandle,
    daemon: State<'_, DaemonController>,
) -> Result<(), String> {
    daemon.stop(&app).await;
    Ok(())
}

// ---------------------------------------------------------------------------
// Read commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn list_workspaces(state: State<'_, AppState>) -> Result<Vec<Workspace>, String> {
    match state.mode {
        ClientMode::Mock => Ok(vec![crate::mock::workspace()]),
        ClientMode::Cli => {
            let status = state.client.status().await.map_err(|e| e.to_string())?;
            let path = state
                .repository
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_default();
            let name = state
                .repository
                .as_ref()
                .and_then(|p| p.file_name())
                .and_then(|n| n.to_str())
                .unwrap_or("repository")
                .to_string();
            Ok(vec![Workspace {
                id: status.workspace_id.clone(),
                name,
                path,
                shared_store_path: String::new(),
                current_branch_id: status.branch.id.clone(),
                current_revision: status.head_revision.id.clone(),
                view: vec![],
                dirty: !status.entries.is_empty(),
                staged_file_count: status.counts.staged,
            }])
        }
    }
}

#[tauri::command]
pub async fn get_workspace_status(
    state: State<'_, AppState>,
) -> Result<WorkspaceStatus, String> {
    state.client.status().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_branches(state: State<'_, AppState>) -> Result<Vec<Branch>, String> {
    match state.mode {
        ClientMode::Mock => Ok(vec![crate::mock::branch()]),
        ClientMode::Cli => {
            let status = state.client.status().await.map_err(|e| e.to_string())?;
            Ok(vec![status.branch])
        }
    }
}

#[tauri::command]
pub async fn list_revisions(
    state: State<'_, AppState>,
    limit: Option<u32>,
) -> Result<Vec<Revision>, String> {
    let mut revs = match state.mode {
        ClientMode::Mock => crate::mock::revisions(),
        // History parsing isn't wired yet; surface at least the head revision.
        ClientMode::Cli => {
            let status = state.client.status().await.map_err(|e| e.to_string())?;
            vec![status.head_revision]
        }
    };
    if let Some(n) = limit {
        revs.truncate(n as usize);
    }
    Ok(revs)
}

#[tauri::command]
pub async fn list_locks(state: State<'_, AppState>) -> Result<Vec<Lock>, String> {
    state.locks.list().await
}

// ---------------------------------------------------------------------------
// Mutating lock commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn acquire_lock(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
    reason: Option<String>,
) -> Result<Lock, String> {
    state.locks.acquire(&app, &path, reason).await
}

#[tauri::command]
pub async fn release_lock(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> Result<(), String> {
    state.locks.release(&app, &path).await
}

#[tauri::command]
pub async fn lock_status(
    state: State<'_, AppState>,
    path: String,
) -> Result<LockState, String> {
    state.locks.status(&path).await
}
