//! Tauri shell: wires the core into commands and events. Keep logic in
//! `recomphub-core`; this crate only adapts it.

mod commands;
mod state;

use recomphub_core::paths::AppDirs;
use recomphub_core::platform;
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

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
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
        ])
        .run(tauri::generate_context!())
        .unwrap_or_else(|err| {
            tracing::error!(%err, "tauri runtime failed");
            std::process::exit(1);
        });
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
