//! Tauri commands. Errors serialize as `{ code, params }` (see `AppError`).

use crate::state::{AppState, RunningJob};
use chrono::{DateTime, Utc};
use recomphub_core::catalog::KnownDump;
use recomphub_core::catalog::{self, CatalogEntry, LoadedCatalog};
use recomphub_core::gamedata::{self, GameDataStatus};
use recomphub_core::install::{self, InstallOptions, InstallRequest, Preflight};
use recomphub_core::installed::{InstalledApp, InstalledDb, InstalledSummary};
use recomphub_core::job::{CancelFlag, Progress};
use recomphub_core::launch;
use recomphub_core::paths::{self, LibraryDirs};
use recomphub_core::settings::Settings;
use recomphub_core::steam::{self, SteamStatus};
use recomphub_core::update::{self, UpdateInfo, UpdateRequest};
use recomphub_core::{download, github, AppError, Result};
use recomphub_core::{platform, shortcuts};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_opener::OpenerExt;

#[derive(Serialize)]
pub struct AppInfo {
    version: &'static str,
    /// This build can update itself (an updater key is configured).
    self_update: bool,
    data_dir: PathBuf,
    library_dir: PathBuf,
    default_library_dir: PathBuf,
}

#[tauri::command]
pub async fn app_info(app: AppHandle, state: State<'_, AppState>) -> Result<AppInfo> {
    let settings = state.settings.read().await;
    let library = state.dirs.library(settings.library_dir.as_deref());
    Ok(AppInfo {
        version: env!("CARGO_PKG_VERSION"),
        self_update: crate::updater_configured(app.config()),
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
    let sources_changed = current.catalog_url != settings.catalog_url
        || current.custom_sources != settings.custom_sources;
    *current = settings.clone();
    drop(current);
    if sources_changed {
        *state.catalog.write().await = None;
    }
    Ok(settings)
}

/// Add a user catalog. It is fetched and checked first, so a typo or a
/// file without valid entries is reported instead of saved.
#[tauri::command]
pub async fn source_add(state: State<'_, AppState>, url: String) -> Result<Settings> {
    let url = url.trim().to_owned();
    recomphub_core::settings::validate_catalog_url("url", &url)?;
    let (entries, warnings) = catalog::load_custom(&state.http, &url).await?;
    if entries.is_empty() {
        return Err(AppError::CatalogInvalid { errors: warnings });
    }
    let mut next = state.settings.read().await.clone();
    if !next.custom_sources.contains(&url) {
        next.custom_sources.push(url);
    }
    settings_set(state, next).await
}

#[tauri::command]
pub async fn source_remove(state: State<'_, AppState>, url: String) -> Result<Settings> {
    let mut next = state.settings.read().await.clone();
    next.custom_sources.retain(|s| s != &url);
    settings_set(state, next).await
}

async fn load_catalog(state: &AppState, force: bool) -> LoadedCatalog {
    let _guard = state.catalog_load.lock().await;
    if !force {
        if let Some(loaded) = state.catalog.read().await.as_ref() {
            return loaded.clone();
        }
    }
    let (url, custom) = {
        let settings = state.settings.read().await;
        (
            settings.catalog_url.clone(),
            settings.custom_sources.clone(),
        )
    };
    let loaded = catalog::load_with_sources(&state.http, &url, &custom).await;
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
        let fresh_install = matches!(kind, JobKind::Install(_));
        let result = run_job(&app, &state, job, &entry, kind, &cancel).await;
        let result = match result {
            Ok(installed) => Ok(integrate(&state, installed, fresh_install).await),
            Err(err) => Err(err),
        };
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

/// Shortcuts after a job: a fresh install gets a start menu link (and a
/// desktop link if the setting is on); an update or restore refreshes the
/// links and Steam entries it already has, since the executable may have
/// changed. Failures here are logged, never fail the job.
async fn integrate(state: &AppState, installed: InstalledApp, fresh_install: bool) -> InstalledApp {
    let platform = platform::current();
    let desktop = state.settings.read().await.desktop_shortcut;
    let result = tokio::task::block_in_place(|| -> Result<InstalledApp> {
        if fresh_install {
            let paths = shortcuts::link_paths(platform, &installed, desktop)?;
            shortcuts::write(platform, &installed, &paths)?;
            return record_shortcuts(state, &installed.id, paths);
        }
        if !installed.shortcuts.is_empty() {
            shortcuts::write(platform, &installed, &installed.shortcuts)?;
        }
        if !installed.steam_users.is_empty() {
            match (platform.steam_dir(), steam::ensure_closed()) {
                (Some(dir), Ok(())) => steam::add(&dir, &installed, &installed.steam_users)?,
                _ => {
                    tracing::warn!(id = %installed.id, "Steam entry not refreshed (Steam open or missing)")
                }
            }
        }
        Ok(installed.clone())
    });
    result.unwrap_or_else(|err| {
        tracing::warn!(id = %installed.id, %err, "shortcuts not updated");
        installed
    })
}

fn record_shortcuts(state: &AppState, id: &str, paths: Vec<PathBuf>) -> Result<InstalledApp> {
    let ((), app) = InstalledDb::modify(&state.dirs.installed_file(), id, |app| {
        for path in paths {
            if !app.shortcuts.contains(&path) {
                app.shortcuts.push(path);
            }
        }
        Ok(())
    })?;
    Ok(app)
}

/// Create a start menu link, and a desktop link if `desktop`.
#[tauri::command]
pub async fn shortcut_create(
    state: State<'_, AppState>,
    id: String,
    desktop: bool,
) -> Result<InstalledSummary> {
    let (installed, _) = installed_app(&state, &id).await?;
    let platform = platform::current();
    tokio::task::block_in_place(|| {
        let paths = shortcuts::link_paths(platform, &installed, desktop)?;
        shortcuts::write(platform, &installed, &paths)?;
        Ok(record_shortcuts(&state, &id, paths)?.summary())
    })
}

/// Remove every link RecompHub created for the app.
#[tauri::command]
pub async fn shortcut_remove(state: State<'_, AppState>, id: String) -> Result<InstalledSummary> {
    let ((), app) = InstalledDb::modify(&state.dirs.installed_file(), &id, |app| {
        let ours: Vec<PathBuf> = app
            .shortcuts
            .drain(..)
            .filter(|p| shortcuts::is_link(p))
            .collect();
        shortcuts::remove(&ours);
        Ok(())
    })?;
    Ok(app.summary())
}

#[tauri::command]
pub async fn steam_status() -> Result<SteamStatus> {
    let dir = platform::current().steam_dir();
    Ok(tokio::task::block_in_place(|| {
        steam::status(dir.as_deref())
    }))
}

/// Add the app to Steam for the given accounts. Steam must be closed.
#[tauri::command]
pub async fn steam_add(
    state: State<'_, AppState>,
    id: String,
    user_ids: Vec<u32>,
) -> Result<InstalledSummary> {
    let (installed, _) = installed_app(&state, &id).await?;
    let dir = platform::current()
        .steam_dir()
        .ok_or(AppError::SteamNotFound)?;
    tokio::task::block_in_place(|| {
        steam::ensure_closed()?;
        steam::add(&dir, &installed, &user_ids)?;
        let ((), app) = InstalledDb::modify(&state.dirs.installed_file(), &id, |app| {
            for user in &user_ids {
                if !app.steam_users.contains(user) {
                    app.steam_users.push(*user);
                }
            }
            Ok(())
        })?;
        Ok(app.summary())
    })
}

/// Remove the app from every Steam account it was added to.
#[tauri::command]
pub async fn steam_remove(state: State<'_, AppState>, id: String) -> Result<InstalledSummary> {
    let (installed, _) = installed_app(&state, &id).await?;
    let dir = platform::current()
        .steam_dir()
        .ok_or(AppError::SteamNotFound)?;
    tokio::task::block_in_place(|| {
        steam::ensure_closed()?;
        steam::remove(&dir, &installed, &installed.steam_users)?;
        let ((), app) = InstalledDb::modify(&state.dirs.installed_file(), &id, |app| {
            app.steam_users.clear();
            Ok(())
        })?;
        Ok(app.summary())
    })
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

async fn installed_app(state: &AppState, id: &str) -> Result<(InstalledApp, CatalogEntry)> {
    let entry = catalog_entry(state, id).await?;
    let app = InstalledDb::load(&state.dirs.installed_file())?
        .get(id)
        .cloned()
        .ok_or_else(|| AppError::NotInstalled { id: id.into() })?;
    Ok((app, entry))
}

#[tauri::command]
pub async fn gamedata_status(state: State<'_, AppState>, id: String) -> Result<GameDataStatus> {
    let (app, entry) = installed_app(&state, &id).await?;
    tokio::task::block_in_place(|| gamedata::status(&app, &entry))
}

/// Check the user's dump at `path` and copy it into the game folder.
#[tauri::command]
pub async fn gamedata_import(
    state: State<'_, AppState>,
    id: String,
    path: PathBuf,
) -> Result<KnownDump> {
    let (app, entry) = installed_app(&state, &id).await?;
    // Not while an update swaps files in the same folder.
    let _slot = state.job_slot.lock().await;
    tokio::task::block_in_place(|| gamedata::import(&app, &entry, &path))
}

#[derive(Clone, Serialize)]
struct GameExitedEvent<'a> {
    id: &'a str,
    code: Option<i32>,
}

/// Start the game. Emits `game:exited` with the exit code when it ends.
#[tauri::command]
pub async fn launch(app: AppHandle, state: State<'_, AppState>, id: String) -> Result<()> {
    let (installed, entry) = installed_app(&state, &id).await?;
    if state
        .jobs
        .lock()
        .expect("jobs lock poisoned")
        .values()
        .any(|j| j.app_id == id)
    {
        return Err(AppError::Busy { id });
    }
    let launched = tokio::task::block_in_place(|| launch::launch(&state.dirs, &installed, &entry))?;
    state
        .running
        .lock()
        .expect("running lock poisoned")
        .insert(id.clone());
    tauri::async_runtime::spawn(async move {
        let code = launched.wait().await;
        tracing::info!(%id, ?code, "game exited");
        app.state::<AppState>()
            .running
            .lock()
            .expect("running lock poisoned")
            .remove(&id);
        let _ = app.emit("game:exited", GameExitedEvent { id: &id, code });
    });
    Ok(())
}

/// Apps started from RecompHub that are still running.
#[tauri::command]
pub async fn running_list(state: State<'_, AppState>) -> Result<Vec<String>> {
    let mut ids: Vec<String> = state
        .running
        .lock()
        .expect("running lock poisoned")
        .iter()
        .cloned()
        .collect();
    ids.sort();
    Ok(ids)
}

/// Open the game's log in the default text editor.
#[tauri::command]
pub async fn open_log(app: AppHandle, state: State<'_, AppState>, id: String) -> Result<()> {
    let path = launch::log_path(&state.dirs, &id);
    if !path.is_file() {
        return Err(AppError::NotFound {
            what: paths::normalize(&path).display().to_string(),
        });
    }
    app.opener()
        .open_path(paths::normalize(&path).to_string_lossy(), None::<&str>)
        .map_err(|e| AppError::InvalidInput {
            message: e.to_string(),
        })
}

/// The end of the game's last log, to show after a crash.
#[tauri::command]
pub async fn log_tail(state: State<'_, AppState>, id: String) -> Result<String> {
    launch::log_tail(&launch::log_path(&state.dirs, &id), 8 * 1024)
}

#[tauri::command]
pub async fn job_cancel(state: State<'_, AppState>, job: u64) -> Result<()> {
    if let Some(running) = state.jobs.lock().expect("jobs lock poisoned").get(&job) {
        running.cancel.cancel();
    }
    Ok(())
}
