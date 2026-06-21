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
use tauri::{AppHandle, Emitter, State};

const LORE_EVENT_CHANNEL: &str = "lore://event";

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

// ---------------------------------------------------------------------------
// Staging & commit
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn stage_files(
    app: AppHandle,
    state: State<'_, AppState>,
    paths: Vec<String>,
) -> Result<(), String> {
    state.client.stage(&paths).await.map_err(|e| e.to_string())?;
    emit(&app, LoreEventTag::StatusChanged, serde_json::json!({ "staged": paths }));
    Ok(())
}

#[tauri::command]
pub async fn unstage_files(
    app: AppHandle,
    state: State<'_, AppState>,
    paths: Vec<String>,
) -> Result<(), String> {
    state.client.unstage(&paths).await.map_err(|e| e.to_string())?;
    emit(&app, LoreEventTag::StatusChanged, serde_json::json!({ "unstaged": paths }));
    Ok(())
}

#[tauri::command]
pub async fn commit(
    app: AppHandle,
    state: State<'_, AppState>,
    message: String,
) -> Result<String, String> {
    let result = state.client.commit(&message).await.map_err(|e| e.to_string())?;
    emit(
        &app,
        LoreEventTag::RevisionCommitted,
        serde_json::json!({ "message": message }),
    );
    Ok(result)
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

// ---------------------------------------------------------------------------
// Phase 4: memory-efficient streaming ingest
// ---------------------------------------------------------------------------

/// Stream a (potentially multi-GB) file into content-addressed fragments,
/// emitting `transferProgress` events. Runs on the async runtime, so the UI
/// thread is never blocked; resident memory is bounded to one chunk.
#[tauri::command]
pub async fn stream_ingest_file(
    app: AppHandle,
    path: String,
) -> Result<crate::streaming::IngestSummary, String> {
    let op_id = format!("ingest-{}", chrono::Utc::now().timestamp_millis());
    crate::streaming::stream_ingest(app, path, op_id).await
}

// ---------------------------------------------------------------------------
// Phase 4: visual diff-tool integration hooks
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn list_diff_tools() -> Vec<crate::diff_tools::DiffToolInfo> {
    crate::diff_tools::list()
}

/// Launch a native diff tool on two arbitrary file paths (the integration hook).
#[tauri::command]
pub fn launch_diff_tool(
    left: String,
    right: String,
    tool_id: Option<String>,
) -> Result<crate::diff_tools::DiffToolInfo, String> {
    crate::diff_tools::launch(tool_id.as_deref(), &left, &right)
}

/// Convenience: diff a repository asset's working copy against its committed
/// base in a native tool. Base export for binary assets is the documented
/// integration point — 0.8.3's CLI has no binary revision export, so until that
/// lands we snapshot the working copy as the base so the tool still opens with
/// the asset. The hook (resolving versions + launching the native tool) is what
/// this demonstrates.
#[tauri::command]
pub fn launch_asset_diff(
    state: State<'_, AppState>,
    path: String,
    tool_id: Option<String>,
) -> Result<crate::diff_tools::DiffToolInfo, String> {
    let repo = state
        .repository
        .as_ref()
        .ok_or_else(|| "no repository on disk (mock backend) — use the diff hook directly".to_string())?;
    let working = repo.join(&path);
    if !working.exists() {
        return Err(format!("asset not found on disk: {}", working.display()));
    }
    // Snapshot the committed base to a temp file (placeholder until binary
    // revision export is available).
    let base = std::env::temp_dir().join(format!(
        "lore-base-{}-{}",
        chrono::Utc::now().timestamp_millis(),
        working.file_name().and_then(|n| n.to_str()).unwrap_or("asset")
    ));
    std::fs::copy(&working, &base).map_err(|e| format!("snapshot base: {e}"))?;

    crate::diff_tools::launch(
        tool_id.as_deref(),
        &base.display().to_string(),
        &working.display().to_string(),
    )
}
