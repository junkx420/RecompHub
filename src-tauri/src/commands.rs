//! Tauri commands. Errors serialize as `{ code, params }` (see `AppError`).

use crate::state::{AppState, RunningJob};
use chrono::{DateTime, Utc};
use recomphub_core::catalog::{self, CatalogEntry, LoadedCatalog};
use recomphub_core::install::{self, InstallOptions, InstallRequest, Preflight};
use recomphub_core::installed::{InstalledApp, InstalledDb, InstalledSummary};
use recomphub_core::job::{CancelFlag, Progress};
use recomphub_core::paths::{self, LibraryDirs};
use recomphub_core::settings::Settings;
use recomphub_core::update::{self, UpdateInfo, UpdateRequest};
use recomphub_core::{download, github, AppError, Result};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, State};

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

async fn catalog_entry(state: &AppState, id: &str) -> Result<CatalogEntry> {
    load_catalog(state, false)
        .await
        .get(id)
        .cloned()
        .ok_or_else(|| AppError::UnknownApp { id: id.into() })
}

async fn library(state: &AppState) -> LibraryDirs {
    let settings = state.settings.read().await;
    state.dirs.library(settings.library_dir.as_deref())
}

#[derive(Serialize)]
pub struct ReleaseInfo {
    tag: String,
    name: Option<String>,
    published_at: Option<DateTime<Utc>>,
    html_url: String,
    asset_name: String,
    asset_size: u64,
    /// GitHub published a SHA-256 for the asset, so the download is verified.
    verified: bool,
}

#[tauri::command]
pub async fn release_info(state: State<'_, AppState>, id: String) -> Result<ReleaseInfo> {
    let entry = catalog_entry(&state, &id).await?;
    let release = github::fetch_release(&state.http, &entry).await?;
    let asset = release.asset_for(&entry)?;
    Ok(ReleaseInfo {
        tag: release.tag_name.clone(),
        name: release.name.clone(),
        published_at: release.published_at,
        html_url: release.html_url.clone(),
        asset_name: asset.name.clone(),
        asset_size: asset.size,
        verified: asset
            .digest
            .as_deref()
            .and_then(download::parse_sha256_digest)
            .is_some(),
    })
}

#[tauri::command]
pub async fn install_preflight(state: State<'_, AppState>, id: String) -> Result<Preflight> {
    let entry = catalog_entry(&state, &id).await?;
    let db = InstalledDb::load(&state.dirs.installed_file())?;
    install::preflight(&library(&state).await, &db, &entry)
}

#[tauri::command]
pub async fn library_list(state: State<'_, AppState>) -> Result<Vec<InstalledSummary>> {
    Ok(InstalledDb::load(&state.dirs.installed_file())?.summaries())
}

#[derive(Clone, Serialize)]
struct JobProgressEvent<'a> {
    job: u64,
    id: &'a str,
    progress: &'a Progress,
}

#[derive(Clone, Serialize)]
struct JobFinishedEvent<'a> {
    job: u64,
    id: &'a str,
    app: InstalledSummary,
}

#[derive(Clone, Serialize)]
struct JobFailedEvent<'a> {
    job: u64,
    id: &'a str,
    error: &'a AppError,
}

enum JobKind {
    Install(InstallOptions),
    Update,
    Restore,
}

/// Start installing `id`. Returns the job id at once; the outcome arrives as
/// `job:progress`, `job:finished` or `job:failed` events.
#[tauri::command]
pub async fn install(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    options: Option<InstallOptions>,
) -> Result<u64> {
    start_job(
        app,
        &state,
        id,
        JobKind::Install(options.unwrap_or_default()),
    )
    .await
}

/// Start updating `id` to its latest release (events as for `install`).
#[tauri::command]
pub async fn update(app: AppHandle, state: State<'_, AppState>, id: String) -> Result<u64> {
    start_job(app, &state, id, JobKind::Update).await
}

/// Start restoring the version before the last update (events as for `install`).
#[tauri::command]
pub async fn restore_previous(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<u64> {
    start_job(app, &state, id, JobKind::Restore).await
}

#[tauri::command]
pub async fn updates_check(state: State<'_, AppState>) -> Result<Vec<UpdateInfo>> {
    let catalog = load_catalog(&state, false).await;
    let db = InstalledDb::load(&state.dirs.installed_file())?;
    Ok(update::check_updates(&state.http, &db, &catalog).await)
}

async fn start_job(app: AppHandle, state: &AppState, id: String, kind: JobKind) -> Result<u64> {
    let entry = catalog_entry(state, &id).await?;
    let cancel = CancelFlag::new();
    let job = {
        let mut jobs = state.jobs.lock().expect("jobs lock poisoned");
        if jobs.values().any(|j| j.app_id == id) {
            return Err(AppError::Busy { id });
        }
        let job = state.next_job.fetch_add(1, Ordering::Relaxed);
        jobs.insert(
            job,
            RunningJob {
                app_id: id.clone(),
                cancel: cancel.clone(),
            },
        );
        job
    };

    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        let result = run_job(&app, &state, job, &entry, kind, &cancel).await;
        state.jobs.lock().expect("jobs lock poisoned").remove(&job);
        let id = entry.id.as_str();
        match result {
            Ok(installed) => {
                let _ = app.emit(
                    "job:finished",
                    JobFinishedEvent {
                        job,
                        id,
                        app: installed.summary(),
                    },
                );
            }
            Err(error) => {
                if !matches!(error, AppError::Cancelled) {
                    tracing::error!(%id, %error, "job failed");
                }
                let _ = app.emit(
                    "job:failed",
                    JobFailedEvent {
                        job,
                        id,
                        error: &error,
                    },
                );
            }
        }
    });
    Ok(job)
}

async fn run_job(
    app: &AppHandle,
    state: &AppState,
    job: u64,
    entry: &CatalogEntry,
    kind: JobKind,
    cancel: &CancelFlag,
) -> Result<InstalledApp> {
    let throttle = ProgressThrottle::default();
    let emit = |progress: Progress| {
        if throttle.should_emit(&progress) {
            let _ = app.emit(
                "job:progress",
                JobProgressEvent {
                    job,
                    id: &entry.id,
                    progress: &progress,
                },
            );
        }
    };
    emit(Progress::Resolving);
    let _slot = state.job_slot.lock().await;
    cancel.check()?;
    match kind {
        JobKind::Install(options) => {
            let release = github::fetch_release(&state.http, entry).await?;
            let lib = library(state).await;
            install::install(InstallRequest {
                http: &state.http,
                lib: &lib,
                installed_file: &state.dirs.installed_file(),
                entry,
                release: &release,
                options,
                cancel,
                progress: &emit,
            })
            .await
        }
        JobKind::Update => {
            let release = github::fetch_release(&state.http, entry).await?;
            update::update(UpdateRequest {
                http: &state.http,
                dirs: &state.dirs,
                entry,
                release: &release,
                cancel,
                progress: &emit,
            })
            .await
        }
        JobKind::Restore => tokio::task::block_in_place(|| {
            update::restore_previous(&state.dirs, entry, cancel, &emit)
        }),
    }
}

/// At most one progress event per 100 ms, but every phase change goes out.
#[derive(Default)]
struct ProgressThrottle(std::sync::Mutex<Option<(Instant, std::mem::Discriminant<Progress>)>>);

impl ProgressThrottle {
    fn should_emit(&self, progress: &Progress) -> bool {
        let phase = std::mem::discriminant(progress);
        let now = Instant::now();
        let mut last = self.0.lock().expect("throttle lock poisoned");
        let due = match *last {
            Some((at, last_phase)) => {
                last_phase != phase || now.duration_since(at) >= Duration::from_millis(100)
            }
            None => true,
        };
        if due {
            *last = Some((now, phase));
        }
        due
    }
}

#[tauri::command]
pub async fn job_cancel(state: State<'_, AppState>, job: u64) -> Result<()> {
    if let Some(running) = state.jobs.lock().expect("jobs lock poisoned").get(&job) {
        running.cancel.cancel();
    }
    Ok(())
}
