//! lore-desktop backend library.
//!
//! Desktop and (future) mobile entrypoints both call `run()`. On startup we
//! discover the Lore configuration (binary + repository), build the active
//! backend, manage shared state, and bring the daemon up; on exit we stop it.

mod commands;
mod daemon;
mod diff_tools;
mod events;
mod fixtures;
mod lock_manager;
mod lore;
mod models;
mod paths;
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
        .plugin(tauri_plugin_dialog::init())
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
            commands::switch_branch,
            commands::create_branch,
            commands::sync_repository,
            commands::push_repository,
            commands::current_identity,
            commands::set_repository,
            commands::clone_repository,
        ])
        .setup(|app| {
            // Start the cross-platform watcher (no-op unless a repository is
            // configured). Safe on all OSes — it no longer depends on the
            // unsupported `lore service run`.
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                handle.state::<DaemonController>().start(&handle).await;
            });
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

// build.rs picks liblore's per-target file layout via include!; re-include it
// here so the default `cargo test --lib` pins the mapping — asset names match
// the real upstream release manifest — without the feature or the library.
#[cfg(test)]
mod liblore_layout_tests {
    include!("../liblore_layout.rs");

    #[test]
    fn asset_names_match_the_upstream_release_manifest() {
        // Names verified against `gh release view v0.8.3 --repo EpicGames/lore`.
        for (target, asset) in [
            ("aarch64-apple-darwin", "liblore-v0.8.3-aarch64-apple-darwin.tar.gz"),
            ("x86_64-unknown-linux-gnu", "liblore-v0.8.3-x86_64-unknown-linux-gnu.tar.gz"),
            (
                "aarch64-unknown-linux-gnu",
                "liblore-v0.8.3-aarch64-unknown-linux-gnu-neoverse-512tvb.tar.gz",
            ),
            ("x86_64-pc-windows-msvc", "liblore-v0.8.3-x86_64-pc-windows-msvc.zip"),
        ] {
            assert_eq!(liblore_layout(target).unwrap().asset, asset, "{target}");
        }
    }

    #[test]
    fn link_and_runtime_files_per_platform() {
        // (link_file, runtime_file, uses_rpath) — verified against the actual
        // archive contents: mac/linux ship one shared library; Windows ships
        // lore.dll + the lore.lib import lib and has no rpath.
        let mac = liblore_layout("aarch64-apple-darwin").unwrap();
        assert_eq!(
            (mac.link_file, mac.runtime_file, mac.uses_rpath),
            ("liblore.dylib", "liblore.dylib", true)
        );
        let linux = liblore_layout("aarch64-unknown-linux-gnu").unwrap();
        assert_eq!(
            (linux.link_file, linux.runtime_file, linux.uses_rpath),
            ("liblore.so", "liblore.so", true)
        );
        let win = liblore_layout("x86_64-pc-windows-msvc").unwrap();
        assert_eq!(
            (win.link_file, win.runtime_file, win.uses_rpath),
            ("lore.lib", "lore.dll", false)
        );
    }

    #[test]
    fn unsupported_targets_point_at_the_cli_backend() {
        for target in ["x86_64-apple-darwin", "riscv64gc-unknown-linux-gnu"] {
            let err = liblore_layout(target).unwrap_err();
            assert!(err.contains("--features liblore"), "{target}: {err}");
        }
    }

    #[test]
    fn fetch_script_mapping_stays_in_sync() {
        // scripts/fetch-liblore.sh duplicates the mapping in shell; parse the
        // script text so the two can't drift silently.
        let script = include_str!("../../scripts/fetch-liblore.sh");
        for target in [
            "aarch64-apple-darwin",
            "x86_64-unknown-linux-gnu",
            "aarch64-unknown-linux-gnu",
            "x86_64-pc-windows-msvc",
        ] {
            let asset = liblore_layout(target).unwrap().asset;
            let suffix = asset
                .strip_prefix(&format!("liblore-{LIBLORE_VERSION}-"))
                .unwrap();
            assert!(
                script.contains(&format!("liblore-${{VERSION}}-{suffix}")),
                "fetch-liblore.sh is missing {suffix} for {target}"
            );
        }
        assert!(
            script.contains(&format!("VERSION=\"{LIBLORE_VERSION}\"")),
            "fetch-liblore.sh pins a different liblore version"
        );
    }
}
