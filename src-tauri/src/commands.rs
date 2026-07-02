//! Tauri IPC command handlers.
//!
//! The signatures are unchanged from Phase 1 — only the bodies switched from
//! static mocks to the live [`LoreClient`] (real CLI or stateful mock, chosen
//! at startup). The frontend is unaffected by that swap, which is the whole
//! point of the trait seam.

use crate::daemon::DaemonController;
use crate::events::{emit, LoreEventKind};
use crate::lore::{ClientMode, LoreError};
use crate::models::*;
use crate::state::AppState;
use tauri::{AppHandle, State};

// ---------------------------------------------------------------------------
// Backend / service introspection
// ---------------------------------------------------------------------------

/// Which backend is active, so the UI can show a "mock data" banner.
#[tauri::command]
pub fn backend_mode(state: State<'_, AppState>) -> ClientMode {
    state.backend().mode
}

#[tauri::command]
pub async fn lore_version(state: State<'_, AppState>) -> Result<String, LoreError> {
    state.backend().client.version().await
}

#[tauri::command]
pub async fn service_state(daemon: State<'_, DaemonController>) -> Result<ServiceState, LoreError> {
    Ok(daemon.state().await)
}

#[tauri::command]
pub async fn start_service(
    app: AppHandle,
    daemon: State<'_, DaemonController>,
) -> Result<(), LoreError> {
    daemon.start(&app).await;
    Ok(())
}

#[tauri::command]
pub async fn stop_service(
    app: AppHandle,
    daemon: State<'_, DaemonController>,
) -> Result<(), LoreError> {
    daemon.stop(&app).await;
    Ok(())
}

// ---------------------------------------------------------------------------
// Read commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn list_workspaces(state: State<'_, AppState>) -> Result<Vec<Workspace>, LoreError> {
    match state.backend().mode {
        ClientMode::Mock => Ok(vec![crate::fixtures::workspace()]),
        // Both live backends derive the workspace from the real status.
        ClientMode::Cli | ClientMode::Ffi => {
            let backend = state.backend();
            let status = backend.client.status().await?;
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
) -> Result<WorkspaceStatus, LoreError> {
    state.backend().client.status().await
}

#[tauri::command]
pub async fn list_branches(state: State<'_, AppState>) -> Result<Vec<Branch>, LoreError> {
    state.backend().client.list_branches().await
}

#[tauri::command]
pub async fn list_revisions(
    state: State<'_, AppState>,
    limit: Option<u32>,
) -> Result<Vec<Revision>, LoreError> {
    state.backend().client.history(limit).await
}

// ---------------------------------------------------------------------------
// VCS workflow (branches, sync, push) + identity
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn switch_branch(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
) -> Result<(), LoreError> {
    state.backend().client.switch_branch(&name).await?;
    emit(&app, LoreEventKind::StatusChanged { source: "switch-branch".into() });
    Ok(())
}

#[tauri::command]
pub async fn create_branch(
    app: AppHandle,
    state: State<'_, AppState>,
    name: String,
) -> Result<(), LoreError> {
    state.backend().client.create_branch(&name).await?;
    emit(&app, LoreEventKind::BranchSwitched { branch: name });
    Ok(())
}

#[tauri::command]
pub async fn sync_repository(
    app: AppHandle,
    state: State<'_, AppState>,
    revision: Option<String>,
) -> Result<(), LoreError> {
    state.backend().client.sync(revision).await?;
    emit(&app, LoreEventKind::StatusChanged { source: "sync".into() });
    Ok(())
}

#[tauri::command]
pub async fn push_repository(
    app: AppHandle,
    state: State<'_, AppState>,
    branch: Option<String>,
) -> Result<(), LoreError> {
    state.backend().client.push(branch).await?;
    emit(&app, LoreEventKind::RevisionCommitted { message: None });
    Ok(())
}

#[tauri::command]
pub async fn current_identity(state: State<'_, AppState>) -> Result<Identity, LoreError> {
    state.backend().client.current_identity().await
}

#[tauri::command]
pub async fn list_locks(state: State<'_, AppState>) -> Result<Vec<Lock>, LoreError> {
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
) -> Result<Lock, LoreError> {
    state.backend().locks.acquire(&app, &path, reason).await
}

#[tauri::command]
pub async fn release_lock(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> Result<(), LoreError> {
    state.backend().locks.release(&app, &path).await
}

#[tauri::command]
pub async fn lock_status(
    state: State<'_, AppState>,
    path: String,
) -> Result<LockState, LoreError> {
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
) -> Result<(), LoreError> {
    state.backend().client.stage(&paths).await?;
    emit(&app, LoreEventKind::StatusChanged { source: "stage".into() });
    Ok(())
}

#[tauri::command]
pub async fn unstage_files(
    app: AppHandle,
    state: State<'_, AppState>,
    paths: Vec<String>,
) -> Result<(), LoreError> {
    state.backend().client.unstage(&paths).await?;
    emit(&app, LoreEventKind::StatusChanged { source: "unstage".into() });
    Ok(())
}

#[tauri::command]
pub async fn commit(
    app: AppHandle,
    state: State<'_, AppState>,
    message: String,
) -> Result<String, LoreError> {
    let result = state.backend().client.commit(&message).await?;
    emit(&app, LoreEventKind::RevisionCommitted { message: Some(message) });
    Ok(result)
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
) -> Result<ClientMode, LoreError> {
    let p = std::path::PathBuf::from(&path);
    if !p.exists() {
        return Err(LoreError::RepositoryNotFound(format!("path does not exist: {path}")));
    }
    let backend = state.set_repository(p.clone());
    daemon.restart(&app, p).await;
    emit(&app, LoreEventKind::StatusChanged { source: "set-repository".into() });
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
) -> Result<ClientMode, LoreError> {
    let dest = std::path::PathBuf::from(&path);

    // FFI-native clone when built with `--features liblore`; otherwise shell out
    // to the `lore` binary. Both create the repository at `path`.
    #[cfg(feature = "liblore")]
    {
        let url = url.clone();
        let dest = dest.clone();
        tokio::task::spawn_blocking(move || crate::lore::ffi::clone(&url, &dest))
            .await
            .map_err(|e| LoreError::Io(e.to_string()))??;
    }
    #[cfg(not(feature = "liblore"))]
    {
        let binary = state
            .binary()
            .ok_or_else(|| LoreError::BinaryUnavailable("lore binary not found; cannot clone".into()))?;
        // `--` terminates option parsing so a caller-supplied URL/path starting
        // with `-` can never be interpreted as a `lore` flag.
        let output = tokio::process::Command::new(&binary)
            .arg("--non-interactive")
            .args(["clone", "--", &url, &path])
            .output()
            .await
            .map_err(|e| LoreError::Io(format!("failed to run lore clone: {e}")))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(LoreError::Cli(format!("clone failed: {}", stderr.trim())));
        }
    }

    let backend = state.set_repository(dest.clone());
    daemon.restart(&app, dest).await;
    emit(&app, LoreEventKind::RevisionCommitted { message: None });
    Ok(backend.mode)
}

// ---------------------------------------------------------------------------
// Phase 4: memory-efficient streaming ingest
// ---------------------------------------------------------------------------

/// Stream a (potentially multi-GB) repository file into content-addressed
/// fragments, emitting `transferProgress` events. Runs on the async runtime,
/// so the UI thread is never blocked; resident memory is bounded to one chunk.
///
/// `path` is repository-relative and validated against the repository root —
/// the webview cannot use this to read arbitrary files on disk.
#[tauri::command]
pub async fn stream_ingest_file(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> Result<crate::streaming::IngestSummary, LoreError> {
    let backend = state.backend();
    let repo = backend
        .repository
        .as_ref()
        .ok_or_else(|| {
            LoreError::NoRepository("no repository on disk (mock backend) — cannot ingest".into())
        })?;
    let resolved = crate::paths::resolve_repo_path(repo, &path)?;
    let op_id = format!("ingest-{}", chrono::Utc::now().timestamp_millis());
    crate::streaming::stream_ingest(app, resolved.display().to_string(), op_id)
        .await
        .map_err(LoreError::Io)
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
) -> Result<crate::diff_tools::DiffToolInfo, LoreError> {
    crate::diff_tools::launch(tool_id.as_deref(), &left, &right)
        .map_err(LoreError::Integration)
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
) -> Result<crate::diff_tools::DiffToolInfo, LoreError> {
    let backend = state.backend();
    let repo = backend
        .repository
        .as_ref()
        .ok_or_else(|| {
            LoreError::NoRepository(
                "no repository on disk (mock backend) — use the diff hook directly".into(),
            )
        })?;
    let working = crate::paths::resolve_repo_path(repo, &path)?;
    // Snapshot the committed base to a temp file (placeholder until binary
    // revision export is available). `tempfile` creates it exclusively with an
    // unpredictable name; `keep()` then detaches it so the diff tool can read
    // it after we return — the tool owns its lifetime from here.
    let file_name = working
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("asset");
    let base = tempfile::Builder::new()
        .prefix("lore-base-")
        .suffix(&format!("-{file_name}"))
        .tempfile()
        .map_err(|e| LoreError::Integration(format!("snapshot base: {e}")))?;
    std::fs::copy(&working, base.path()).map_err(|e| LoreError::Integration(format!("snapshot base: {e}")))?;
    let (_file, base_path) = base
        .keep()
        .map_err(|e| LoreError::Integration(format!("snapshot base: {e}")))?;

    crate::diff_tools::launch(
        tool_id.as_deref(),
        &base_path.display().to_string(),
        &working.display().to_string(),
    )
    .map_err(LoreError::Integration)
}
