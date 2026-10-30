//! Tauri commands. Errors serialize as `{ code, params }` (see `AppError`).

use crate::state::AppState;
use recomphub_core::catalog::{self, LoadedCatalog};
use recomphub_core::paths;
use recomphub_core::settings::Settings;
use recomphub_core::Result;
use serde::Serialize;
use std::path::PathBuf;
use tauri::State;

#[derive(Serialize)]
pub struct AppInfo {
    version: &'static str,
    data_dir: PathBuf,
    library_dir: PathBuf,
    default_library_dir: PathBuf,
}

#[tauri::command]
pub async fn app_info(state: State<'_, AppState>) -> Result<AppInfo> {
    let settings = state.settings.read().await;
    let library = state.dirs.library(settings.library_dir.as_deref());
    Ok(AppInfo {
        version: env!("CARGO_PKG_VERSION"),
        data_dir: paths::normalize(state.dirs.root()),
        library_dir: paths::normalize(&library.apps),
        default_library_dir: paths::normalize(&state.dirs.default_library()),
    })
}

#[tauri::command]
pub async fn settings_get(state: State<'_, AppState>) -> Result<Settings> {
    Ok(state.settings.read().await.clone())
}

#[tauri::command]
pub async fn settings_set(state: State<'_, AppState>, settings: Settings) -> Result<Settings> {
    settings.validate()?;
    let mut current = state.settings.write().await;
    settings.save(&state.dirs.settings_file())?;
    let url_changed = current.catalog_url != settings.catalog_url;
    *current = settings.clone();
    drop(current);
    if url_changed {
        *state.catalog.write().await = None;
    }
    Ok(settings)
}

async fn load_catalog(state: &AppState, force: bool) -> LoadedCatalog {
    let _guard = state.catalog_load.lock().await;
    if !force {
        if let Some(loaded) = state.catalog.read().await.as_ref() {
            return loaded.clone();
        }
    }
    let url = state.settings.read().await.catalog_url.clone();
    let loaded = catalog::load(&state.http, &url).await;
    *state.catalog.write().await = Some(loaded.clone());
    loaded
}

#[tauri::command]
pub async fn catalog_get(state: State<'_, AppState>) -> Result<LoadedCatalog> {
    Ok(load_catalog(&state, false).await)
}

#[tauri::command]
pub async fn catalog_refresh(state: State<'_, AppState>) -> Result<LoadedCatalog> {
    Ok(load_catalog(&state, true).await)
}
