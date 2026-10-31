//! Tauri shell: wires the core into commands and events. Keep logic in
//! `recomphub-core`; this crate only adapts it.

mod commands;
mod state;

use recomphub_core::installed::InstalledDb;
use recomphub_core::paths::AppDirs;
use recomphub_core::{platform, update};
use state::AppState;
use tauri::Manager;

pub fn run() {
    let dirs = match AppDirs::from_platform(platform::current()) {
        Ok(dirs) => dirs,
        Err(err) => {
            eprintln!("cannot determine data directory: {err}");
            std::process::exit(1);
        }
    };
    if let Err(err) = dirs.ensure() {
        eprintln!("cannot create data directory: {err}");
        std::process::exit(1);
    }
    let _log_guard = init_logging(&dirs);
    tracing::info!(version = env!("CARGO_PKG_VERSION"), root = ?dirs.root(), "starting RecompHub");

    let state = match AppState::new(dirs) {
        Ok(state) => state,
        Err(err) => {
            tracing::error!(%err, "startup failed");
            std::process::exit(1);
        }
    };
    recover_interrupted_jobs(&state);

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            // Builds without a signing key (development, forks) ship no
            // updater; the plugin would refuse to start without a pubkey.
            if updater_configured(app.config()) {
                app.handle()
                    .plugin(tauri_plugin_updater::Builder::new().build())?;
            } else {
                tracing::info!("self-update disabled: no updater pubkey in tauri.conf.json");
            }
            Ok(())
        })
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::settings_get,
            commands::settings_set,
            commands::catalog_get,
            commands::catalog_refresh,
            commands::release_info,
            commands::install_preflight,
            commands::install,
            commands::job_cancel,
            commands::library_list,
            commands::update,
            commands::restore_previous,
            commands::updates_check,
            commands::gamedata_status,
            commands::gamedata_import,
            commands::launch,
            commands::running_list,
            commands::open_log,
            commands::log_tail,
            commands::shortcut_create,
            commands::shortcut_remove,
            commands::steam_status,
            commands::steam_add,
            commands::steam_remove,
            commands::source_add,
            commands::source_remove,
        ])
        .run(tauri::generate_context!())
        .unwrap_or_else(|err| {
            tracing::error!(%err, "tauri runtime failed");
            std::process::exit(1);
        });
}

/// Is a public key for signed self-updates configured?
pub(crate) fn updater_configured(config: &tauri::Config) -> bool {
    config
        .plugins
        .0
        .get("updater")
        .and_then(|u| u.get("pubkey"))
        .and_then(|k| k.as_str())
        .is_some_and(|k| !k.trim().is_empty())
}

/// Undo updates a crash interrupted, before the UI can start new jobs. Looks
/// in the current library and in every library an installed app lives in.
fn recover_interrupted_jobs(state: &AppState) {
    let dirs = &state.dirs;
    let settings = state.settings.blocking_read();
    let mut libraries = vec![
        dirs.library(None),
        dirs.library(settings.library_dir.as_deref()),
    ];
    match InstalledDb::load(&dirs.installed_file()) {
        Ok(db) => libraries.extend(db.apps.values().map(|a| dirs.library_containing(&a.dir))),
        Err(err) => tracing::error!(%err, "cannot read installed.json for recovery"),
    }
    for line in update::recover(dirs, &libraries) {
        tracing::warn!("recovery: {line}");
    }
}

fn init_logging(dirs: &AppDirs) -> tracing_appender::non_blocking::WorkerGuard {
    use tracing_subscriber::{fmt, prelude::*, EnvFilter};
    let file = tracing_appender::rolling::daily(dirs.logs(), "recomphub.log");
    let (writer, guard) = tracing_appender::non_blocking(file);
    let filter =
        EnvFilter::try_from_env("RECOMPHUB_LOG").unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_writer(writer).with_ansi(false))
        .with(fmt::layer().with_writer(std::io::stderr))
        .init();
    guard
}
