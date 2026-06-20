//! lore-desktop backend library.
//!
//! Desktop and (future) mobile entrypoints both call `run()`. On startup we
//! discover the Lore configuration (binary + repository), build the active
//! backend, manage shared state, and bring the daemon up; on exit we stop it.

mod commands;
mod daemon;
mod diff_tools;
mod lock_manager;
mod lore;
mod mock;
mod models;
mod state;
mod streaming;

use daemon::DaemonController;
use lore::LoreConfig;
use state::AppState;
use tauri::{Manager, RunEvent};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let config = LoreConfig::discover();
    log::info!(
        "lore-desktop: binary={:?} repository={:?}",
        config.binary,
        config.repository
    );

    let app_state = AppState::from_config(&config);
    let daemon = DaemonController::new(config);

    let app = tauri::Builder::default()
        .manage(app_state)
        .manage(daemon)
        .invoke_handler(tauri::generate_handler![
            commands::backend_mode,
            commands::lore_version,
            commands::service_state,
            commands::start_service,
            commands::stop_service,
            commands::list_workspaces,
            commands::get_workspace_status,
            commands::list_branches,
            commands::list_revisions,
            commands::list_locks,
            commands::acquire_lock,
            commands::release_lock,
            commands::lock_status,
            commands::stage_files,
            commands::unstage_files,
            commands::commit,
            commands::stream_ingest_file,
            commands::list_diff_tools,
            commands::launch_diff_tool,
            commands::launch_asset_diff,
        ])
        .setup(|app| {
            // Auto-start the daemon only when explicitly requested. `lore
            // service run` isn't supported on every OS, and CLI lock/status
            // ops don't need it — so default off and let the UI start it on
            // demand via `start_service`.
            if std::env::var_os("LORE_AUTOSTART_SERVICE").is_some() {
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    handle.state::<DaemonController>().start(&handle).await;
                });
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building lore-desktop");

    app.run(|handle, event| {
        if let RunEvent::Exit = event {
            // Graceful daemon shutdown on app exit (cross-platform).
            let handle = handle.clone();
            tauri::async_runtime::block_on(async move {
                handle.state::<DaemonController>().stop(&handle).await;
            });
        }
    });
}
