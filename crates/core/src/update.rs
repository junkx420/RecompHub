//! Updates and "restore previous version", without losing user data.
//!
//! An update swaps files in place instead of replacing the whole folder:
//!
//! - Files the old release placed (`InstalledApp::files`) move to a backup
//!   folder, unless they match a `preserve` glob.
//! - Files of the new release move in where the path is free (or was freed
//!   by the step above). Anything else in the folder (saves, configs, game
//!   data the port created or the user added) is never touched, even if the
//!   new release ships a file with the same name.
//!
//! All moves are renames on the library's volume. Before the first move the
//! whole plan is written to `<staging>/<id>.journal.json`. If a move fails,
//! every move done so far is reversed. If RecompHub dies mid-way, [`recover`]
//! does the same on the next start, unless `installed.json` already carries
//! the journal's revision, which means the update was committed.
//!
//! Restoring the previous version runs the same swap with the backup as the
//! source; the current version becomes the new backup. One backup per app is
//! kept.

use crate::catalog::{CatalogEntry, LoadedCatalog};
use crate::error::{AppError, IoContext, Result};
use crate::fsutil;
use crate::github::{self, Release};
use crate::http::Http;
use crate::install::{self, stamp};
use crate::installed::{InstalledApp, InstalledDb, PreviousVersion};
use crate::job::{CancelFlag, Progress, ProgressFn};
use crate::paths::{AppDirs, LibraryDirs};
use crate::process;
use chrono::{DateTime, Utc};
use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

const JOURNAL_SUFFIX: &str = ".journal.json";
const BACKUP_INFO: &str = "backup.json";

/// Everything needed to finish or undo a swap after a crash.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct Journal {
    id: String,
    /// Revision `installed.json` gets when the swap is committed.
    revision: String,
    app_dir: PathBuf,
    /// Where the new files come from.
    incoming: PathBuf,
    /// Backup folder created for this swap (holds `backup.json`, `files/`).
    backup_dir: PathBuf,
    to_backup: Vec<String>,
    to_place: Vec<String>,
}

impl Journal {
    fn backup_files(&self) -> PathBuf {
        self.backup_dir.join("files")
    }
}

/// What a backup folder holds, so it can be restored.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct BackupInfo {
    version: String,
    asset_name: String,
    asset_id: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    asset_sha256: Option<String>,
    exe: String,
    /// Files in `files/`, with the hashes the release had.
    files: BTreeMap<String, String>,
    created_at: DateTime<Utc>,
}

fn preserve_set(entry: &CatalogEntry) -> Result<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    for pattern in &entry.preserve {
        let glob = GlobBuilder::new(&pattern.replace('\\', "/"))
            .literal_separator(true)
            .case_insensitive(true)
            .build()
            .map_err(|e| AppError::InvalidInput {
                message: format!("preserve {pattern:?}: {e}"),
            })?;
        builder.add(glob);
    }
    builder.build().map_err(|e| AppError::InvalidInput {
        message: format!("preserve: {e}"),
    })
}

/// Which files to move out and which to move in.
fn plan(
    app_dir: &Path,
    old_files: &BTreeMap<String, String>,
    incoming_files: &BTreeMap<String, String>,
    preserve: &GlobSet,
) -> (Vec<String>, Vec<String>) {
    let on_disk = |rel: &str| app_dir.join(rel).is_file();
    let to_backup: Vec<String> = old_files
        .keys()
        .filter(|rel| !preserve.is_match(rel.as_str()) && on_disk(rel))
        .cloned()
        .collect();
    let freed: HashSet<&str> = to_backup.iter().map(String::as_str).collect();
    let to_place = incoming_files
        .keys()
        .filter(|rel| !on_disk(rel) || freed.contains(rel.as_str()))
        .cloned()
        .collect();
    (to_backup, to_place)
}

fn journal_path(lib: &LibraryDirs, id: &str) -> PathBuf {
    lib.staging.join(format!("{id}{JOURNAL_SUFFIX}"))
}

/// Run the journal's moves. On failure, undo them and return the error.
fn apply(journal: &Journal, cancel: &CancelFlag, progress: ProgressFn<'_>) -> Result<()> {
    let total = (journal.to_backup.len() + journal.to_place.len()) as u64;
    let mut done = 0u64;
    let mut step = |from: PathBuf, to: PathBuf| -> Result<()> {
        cancel.check()?;
        fsutil::rename_retry(&from, &to)?;
        done += 1;
        progress(Progress::Applying { done, total });
        Ok(())
    };
    let result = (|| {
        for rel in &journal.to_backup {
            step(journal.app_dir.join(rel), journal.backup_files().join(rel))?;
        }
        for rel in &journal.to_place {
            step(journal.incoming.join(rel), journal.app_dir.join(rel))?;
        }
        Ok(())
    })();
    if let Err(err) = &result {
        tracing::error!(id = %journal.id, %err, "swap failed, rolling back");
        rollback(journal);
    }
    result
}

/// Undo whatever part of the journal was done. Each file's state tells
/// whether it was moved, so this is safe to run any number of times.
fn rollback(journal: &Journal) {
    let undo = |from: PathBuf, to: PathBuf| {
        if from.is_file() && !to.exists() {
            if let Err(err) = fsutil::rename_retry(&from, &to) {
                tracing::error!(?from, ?to, %err, "rollback could not move file back");
            }
        }
    };
    for rel in journal.to_place.iter().rev() {
        undo(journal.app_dir.join(rel), journal.incoming.join(rel));
    }
    for rel in journal.to_backup.iter().rev() {
        undo(journal.backup_files().join(rel), journal.app_dir.join(rel));
    }
}

/// Record `app` in `installed.json`, which commits the swap, then drop the
/// journal. If the record cannot be written, the swap is undone.
fn commit(installed_file: &Path, app: &InstalledApp, journal: &Journal, path: &Path) -> Result<()> {
    let saved = InstalledDb::load(installed_file).and_then(|mut db| {
        db.apps.insert(app.id.clone(), app.clone());
        db.save(installed_file)
    });
    if let Err(err) = saved {
        rollback(journal);
        let _ = std::fs::remove_dir_all(&journal.backup_dir);
        let _ = std::fs::remove_file(path);
        return Err(err);
    }
    let _ = std::fs::remove_file(path);
    Ok(())
}

fn ensure_not_running(dir: &Path) -> Result<()> {
    let processes = process::running_in(dir);
    if processes.is_empty() {
        Ok(())
    } else {
        Err(AppError::GameRunning { processes })
    }
}

fn folder_name(version: &str) -> String {
    version
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || "._-".contains(c) {
                c
            } else {
                '_'
            }
        })
        .collect()
}

struct Swap<'a> {
    app: &'a InstalledApp,
    lib: &'a LibraryDirs,
    preserve: &'a GlobSet,
    incoming: &'a Path,
    incoming_files: &'a BTreeMap<String, String>,
    cancel: &'a CancelFlag,
    progress: ProgressFn<'a>,
}

struct Swapped {
    journal: Journal,
    journal_path: PathBuf,
    placed: BTreeMap<String, String>,
}

/// Back up the current release files and move the incoming ones in.
fn swap(s: Swap<'_>) -> Result<Swapped> {
    let revision = stamp();
    let backup_dir = s
        .lib
        .backups
        .join(&s.app.id)
        .join(format!("{}-{revision}", folder_name(&s.app.version)));
    let (to_backup, to_place) = plan(&s.app.dir, &s.app.files, s.incoming_files, s.preserve);

    let info = BackupInfo {
        version: s.app.version.clone(),
        asset_name: s.app.asset_name.clone(),
        asset_id: s.app.asset_id,
        asset_sha256: s.app.asset_sha256.clone(),
        exe: s.app.exe.clone(),
        files: to_backup
            .iter()
            .filter_map(|rel| Some((rel.clone(), s.app.files.get(rel)?.clone())))
            .collect(),
        created_at: Utc::now(),
    };
    std::fs::create_dir_all(&backup_dir).at(&backup_dir)?;
    fsutil::write_json(&backup_dir.join(BACKUP_INFO), &info)?;

    let journal = Journal {
        id: s.app.id.clone(),
        revision,
        app_dir: s.app.dir.clone(),
        incoming: s.incoming.to_path_buf(),
        backup_dir,
        to_backup,
        to_place,
    };
    let journal_path = journal_path(s.lib, &s.app.id);
    fsutil::write_json(&journal_path, &journal)?;

    if let Err(err) = apply(&journal, s.cancel, s.progress) {
        let _ = std::fs::remove_dir_all(&journal.backup_dir);
        let _ = std::fs::remove_file(&journal_path);
        return Err(err);
    }
    let placed = journal
        .to_place
        .iter()
        .filter_map(|rel| Some((rel.clone(), s.incoming_files.get(rel)?.clone())))
        .collect();
    Ok(Swapped {
        journal,
        journal_path,
        placed,
    })
}

/// Drop a backup folder that is no longer referenced.
fn remove_backup(previous: Option<&PreviousVersion>) {
    if let Some(previous) = previous {
        if let Err(err) = std::fs::remove_dir_all(&previous.dir) {
            tracing::warn!(dir = ?previous.dir, %err, "could not remove old backup");
        }
    }
}

pub struct UpdateRequest<'a> {
    pub http: &'a Http,
    pub dirs: &'a AppDirs,
    pub entry: &'a CatalogEntry,
    pub release: &'a Release,
    pub cancel: &'a CancelFlag,
    pub progress: ProgressFn<'a>,
}

/// Update an installed app to `release`, keeping user data.
pub async fn update(req: UpdateRequest<'_>) -> Result<InstalledApp> {
    let UpdateRequest {
        http,
        dirs,
        entry,
        release,
        cancel,
        progress,
    } = req;
    let installed_file = dirs.installed_file();
    let db = InstalledDb::load(&installed_file)?;
    let app = db
        .get(&entry.id)
        .cloned()
        .ok_or_else(|| AppError::NotInstalled {
            id: entry.id.clone(),
        })?;
    if app.version == release.tag_name {
        return Err(AppError::UpToDate {
            id: app.id,
            version: app.version,
        });
    }
    let lib = dirs.library_containing(&app.dir);
    let preserve = preserve_set(entry)?;
    ensure_not_running(&app.dir)?;

    let prepared = install::prepare(http, &lib, entry, release, cancel, progress).await?;
    let result = tokio::task::block_in_place(|| -> Result<InstalledApp> {
        // The game may have been started during the download.
        ensure_not_running(&app.dir)?;
        let swapped = swap(Swap {
            app: &app,
            lib: &lib,
            preserve: &preserve,
            incoming: &prepared.root,
            incoming_files: &prepared.files,
            cancel,
            progress,
        })?;
        progress(Progress::Finalizing);
        let updated = InstalledApp {
            id: app.id.clone(),
            name: entry.name.clone(),
            version: release.tag_name.clone(),
            asset_name: prepared.asset_name.clone(),
            asset_id: prepared.asset_id,
            asset_sha256: prepared.asset_sha256.clone(),
            dir: app.dir.clone(),
            exe: prepared.exe.clone(),
            installed_at: app.installed_at,
            updated_at: Some(Utc::now()),
            files: swapped.placed,
            previous: Some(PreviousVersion {
                version: app.version.clone(),
                dir: swapped.journal.backup_dir.clone(),
            }),
            revision: swapped.journal.revision.clone(),
            shortcuts: app.shortcuts.clone(),
            steam_users: app.steam_users.clone(),
        };
        commit(
            &installed_file,
            &updated,
            &swapped.journal,
            &swapped.journal_path,
        )?;
        remove_backup(app.previous.as_ref());
        Ok(updated)
    });
    let _ = tokio::fs::remove_dir_all(&prepared.work).await;
    let updated = result?;
    tracing::info!(id = %updated.id, from = %app.version, to = %updated.version, "updated");
    Ok(updated)
}

/// Swap back to the version before the last update. The current version
/// becomes the new backup, so a restore can itself be undone.
pub fn restore_previous(
    dirs: &AppDirs,
    entry: &CatalogEntry,
    cancel: &CancelFlag,
    progress: ProgressFn<'_>,
) -> Result<InstalledApp> {
    let installed_file = dirs.installed_file();
    let db = InstalledDb::load(&installed_file)?;
    let app = db
        .get(&entry.id)
        .cloned()
        .ok_or_else(|| AppError::NotInstalled {
            id: entry.id.clone(),
        })?;
    let no_previous = || AppError::NoPreviousVersion { id: app.id.clone() };
    let previous = app.previous.clone().ok_or_else(no_previous)?;
    let info: BackupInfo = std::fs::read(previous.dir.join(BACKUP_INFO))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .ok_or_else(no_previous)?;
    let lib = dirs.library_containing(&app.dir);
    let preserve = preserve_set(entry)?;
    ensure_not_running(&app.dir)?;

    let swapped = swap(Swap {
        app: &app,
        lib: &lib,
        preserve: &preserve,
        incoming: &previous.dir.join("files"),
        incoming_files: &info.files,
        cancel,
        progress,
    })?;
    progress(Progress::Finalizing);
    let restored = InstalledApp {
        id: app.id.clone(),
        name: entry.name.clone(),
        version: info.version,
        asset_name: info.asset_name,
        asset_id: info.asset_id,
        asset_sha256: info.asset_sha256,
        dir: app.dir.clone(),
        exe: info.exe,
        installed_at: app.installed_at,
        updated_at: Some(Utc::now()),
        files: swapped.placed,
        previous: Some(PreviousVersion {
            version: app.version.clone(),
            dir: swapped.journal.backup_dir.clone(),
        }),
        revision: swapped.journal.revision.clone(),
        shortcuts: app.shortcuts.clone(),
        steam_users: app.steam_users.clone(),
    };
    commit(
        &installed_file,
        &restored,
        &swapped.journal,
        &swapped.journal_path,
    )?;
    // Whatever the restore did not place (shadowed by user files) goes too.
    remove_backup(Some(&previous));
    tracing::info!(id = %restored.id, from = %app.version, to = %restored.version, "restored previous version");
    Ok(restored)
}

/// Finish or undo swaps interrupted by a crash, and clear stale work
/// folders. Call at startup, before any job runs. Returns what was done,
/// for the log.
pub fn recover(dirs: &AppDirs, libraries: &[LibraryDirs]) -> Vec<String> {
    let mut report = Vec::new();
    let installed_file = dirs.installed_file();
    let mut seen = HashSet::new();
    for lib in libraries {
        if !seen.insert(lib.staging.clone()) {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&lib.staging) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.ends_with(JOURNAL_SUFFIX) {
                report.push(recover_journal(&installed_file, &path));
            } else if path.is_dir() && name != "downloads" {
                // A work folder from a job that never finished.
                let _ = std::fs::remove_dir_all(&path);
                report.push(format!("removed stale work folder {name}"));
            }
        }
    }
    report
}

fn recover_journal(installed_file: &Path, path: &Path) -> String {
    let journal: Journal = match std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
    {
        Some(journal) => journal,
        None => {
            // Unreadable: written partially before any file moved.
            let _ = std::fs::remove_file(path);
            return format!("removed unreadable journal {path:?}");
        }
    };
    let committed = InstalledDb::load(installed_file)
        .ok()
        .and_then(|db| db.get(&journal.id).map(|a| a.revision == journal.revision))
        .unwrap_or(false);
    if committed {
        let _ = std::fs::remove_file(path);
        return format!("{}: update was committed, journal removed", journal.id);
    }
    rollback(&journal);
    let _ = std::fs::remove_dir_all(&journal.backup_dir);
    let _ = std::fs::remove_file(path);
    format!("{}: interrupted update rolled back", journal.id)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UpdateInfo {
    pub id: String,
    pub installed: String,
    pub latest: String,
}

/// Installed apps whose catalog release has a different tag. Entries whose
/// release cannot be fetched are skipped with a warning.
pub async fn check_updates(
    http: &Http,
    db: &InstalledDb,
    catalog: &LoadedCatalog,
) -> Vec<UpdateInfo> {
    let mut updates = Vec::new();
    for app in db.apps.values() {
        let Some(entry) = catalog.get(&app.id) else {
            continue;
        };
        match github::fetch_release(http, entry).await {
            Ok(release) if release.tag_name != app.version => updates.push(UpdateInfo {
                id: app.id.clone(),
                installed: app.version.clone(),
                latest: release.tag_name,
            }),
            Ok(_) => {}
            Err(err) => tracing::warn!(id = %app.id, %err, "update check failed"),
        }
    }
    updates
}

#[cfg(test)]
mod tests;
