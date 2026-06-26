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
    state.backend().mode
}

#[tauri::command]
pub async fn lore_version(state: State<'_, AppState>) -> Result<String, String> {
    state.backend().client.version().await.map_err(|e| e.to_string())
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
    match state.backend().mode {
        ClientMode::Mock => Ok(vec![crate::mock::workspace()]),
        ClientMode::Cli => {
            let backend = state.backend();
            let status = backend.client.status().await.map_err(|e| e.to_string())?;
            let path = backend
                .repository
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_default();
            let name = backend
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
    state.backend().client.status().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_branches(state: State<'_, AppState>) -> Result<Vec<Branch>, String> {
    state.backend().client.list_branches().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_revisions(
    state: State<'_, AppState>,
    limit: Option<u32>,
) -> Result<Vec<Revision>, String> {
    state.backend().client.history(limit).await.map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// VCS workflow (branches, sync, push) + identity
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn switch_branch(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
) -> Result<(), String> {
    state.backend().client.switch_branch(&name).await.map_err(|e| e.to_string())?;
    emit(&app, LoreEventTag::StatusChanged, serde_json::json!({ "branch": name }));
    Ok(())
}

#[tauri::command]
pub async fn create_branch(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
) -> Result<(), String> {
    state.backend().client.create_branch(&name).await.map_err(|e| e.to_string())?;
    emit(&app, LoreEventTag::BranchSwitched, serde_json::json!({ "created": name }));
    Ok(())
}

#[tauri::command]
pub async fn sync_repository(
    app: AppHandle,
    state: State<'_, AppState>,
    revision: Option<String>,
) -> Result<(), String> {
    state.backend().client.sync(revision).await.map_err(|e| e.to_string())?;
    emit(&app, LoreEventTag::StatusChanged, serde_json::json!({ "synced": true }));
    Ok(())
}

#[tauri::command]
pub async fn push_repository(
    app: AppHandle,
    state: State<'_, AppState>,
    branch: Option<String>,
) -> Result<(), String> {
    state.backend().client.push(branch).await.map_err(|e| e.to_string())?;
    emit(&app, LoreEventTag::RevisionCommitted, serde_json::json!({ "pushed": true }));
    Ok(())
}

#[tauri::command]
pub async fn current_identity(state: State<'_, AppState>) -> Result<Identity, String> {
    state.backend().client.current_identity().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_locks(state: State<'_, AppState>) -> Result<Vec<Lock>, String> {
    state.backend().locks.list().await
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
    state.backend().locks.acquire(&app, &path, reason).await
}

#[tauri::command]
pub async fn release_lock(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> Result<(), String> {
    state.backend().locks.release(&app, &path).await
}

#[tauri::command]
pub async fn lock_status(
    state: State<'_, AppState>,
    path: String,
) -> Result<LockState, String> {
    state.backend().locks.status(&path).await
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
    state.backend().client.stage(&paths).await.map_err(|e| e.to_string())?;
    emit(&app, LoreEventTag::StatusChanged, serde_json::json!({ "staged": paths }));
    Ok(())
}

#[tauri::command]
pub async fn unstage_files(
    app: AppHandle,
    state: State<'_, AppState>,
    paths: Vec<String>,
) -> Result<(), String> {
    state.backend().client.unstage(&paths).await.map_err(|e| e.to_string())?;
    emit(&app, LoreEventTag::StatusChanged, serde_json::json!({ "unstaged": paths }));
    Ok(())
}

#[tauri::command]
pub async fn commit(
    app: AppHandle,
    state: State<'_, AppState>,
    message: String,
) -> Result<String, String> {
    let result = state.backend().client.commit(&message).await.map_err(|e| e.to_string())?;
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
// Repository management (A4): open / clone at runtime
// ---------------------------------------------------------------------------

/// Switch the active backend to an existing repository on disk, rebuilding the
/// client and re-pointing the watcher. The frontend re-bootstraps afterwards.
#[tauri::command]
pub async fn set_repository(
    app: AppHandle,
    state: State<'_, AppState>,
    daemon: State<'_, DaemonController>,
    path: String,
) -> Result<ClientMode, String> {
    let p = std::path::PathBuf::from(&path);
    if !p.exists() {
        return Err(format!("path does not exist: {path}"));
    }
    let backend = state.set_repository(p.clone());
    daemon.restart(&app, p).await;
    emit(&app, LoreEventTag::StatusChanged, serde_json::json!({ "repository": path }));
    Ok(backend.mode)
}

/// Clone a remote repository into `path` (`lore clone <url> <path>`), then make
/// it the active repository.
#[tauri::command]
pub async fn clone_repository(
    app: AppHandle,
    state: State<'_, AppState>,
    daemon: State<'_, DaemonController>,
    url: String,
    path: String,
) -> Result<ClientMode, String> {
    let binary = state
        .binary()
        .ok_or_else(|| "lore binary not found; cannot clone".to_string())?;
    let output = tokio::process::Command::new(&binary)
        .arg("--non-interactive")
        .args(["clone", &url, &path])
        .output()
        .await
        .map_err(|e| format!("failed to run lore clone: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("clone failed: {}", stderr.trim()));
    }
    let dest = std::path::PathBuf::from(&path);
    let backend = state.set_repository(dest.clone());
    daemon.restart(&app, dest).await;
    emit(&app, LoreEventTag::RevisionCommitted, serde_json::json!({ "cloned": url }));
    Ok(backend.mode)
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
    let backend = state.backend();
    let repo = backend
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
