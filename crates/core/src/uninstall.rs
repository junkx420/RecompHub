//! Uninstalling a port, with or without the user's data.
//!
//! The game folder is first renamed into staging, which fails cleanly if a
//! file is locked, so a failed uninstall leaves the game as it was.
//!
//! With `keep_saves`, everything the release did not place (saves, configs,
//! imported game data, mods) plus files matching `preserve` moves to
//! `<backups>\<id>\saves-<time>`, and the next install of the same id moves
//! it back. Without it, the entry's `user_data_paths` outside the game
//! folder are deleted too. Shortcuts and Steam entries are the caller's
//! job, since they need the platform.

use crate::archive;
use crate::catalog::CatalogEntry;
use crate::error::{AppError, Result};
use crate::fsutil;
use crate::install::stamp;
use crate::installed::{InstalledApp, InstalledDb};
use crate::paths::{AppDirs, LibraryDirs};
use crate::update;
use globset::GlobSet;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const SAVES_PREFIX: &str = "saves-";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UninstallOutcome {
    /// Where kept saves went, if there were any.
    pub kept_saves: Option<PathBuf>,
    /// Save folders outside the game folder that were deleted.
    pub removed_user_data: Vec<PathBuf>,
}

/// `%LOCALAPPDATA%\X`, `%APPDATA%\X` or `%USERPROFILE%\X` from the catalog,
/// resolved with the current environment.
pub fn expand_user_path(path: &str) -> Option<PathBuf> {
    for (var, prefix) in [
        ("LOCALAPPDATA", "%LOCALAPPDATA%\\"),
        ("APPDATA", "%APPDATA%\\"),
        ("USERPROFILE", "%USERPROFILE%\\"),
    ] {
        if let Some(rest) = path.strip_prefix(prefix) {
            if rest.is_empty() || rest.split('\\').any(|s| s.is_empty() || s == "..") {
                return None;
            }
            return Some(PathBuf::from(std::env::var_os(var)?).join(rest));
        }
    }
    None
}

/// Files in `dir` that belong to the user rather than to the release.
fn user_files(
    dir: &Path,
    manifest: &BTreeMap<String, String>,
    preserve: &GlobSet,
) -> Result<Vec<String>> {
    Ok(archive::list_files(dir)?
        .into_iter()
        .filter(|rel| !manifest.contains_key(rel) || preserve.is_match(rel.as_str()))
        .collect())
}

/// Move `files` from `from` to `to`; on failure, move the done ones back.
fn move_files(files: &[String], from: &Path, to: &Path) -> Result<()> {
    for (i, rel) in files.iter().enumerate() {
        if let Err(err) = fsutil::rename_retry(&from.join(rel), &to.join(rel)) {
            for done in files[..i].iter().rev() {
                let _ = fsutil::rename_retry(&to.join(done), &from.join(done));
            }
            return Err(err);
        }
    }
    Ok(())
}

pub struct UninstallRequest<'a> {
    pub dirs: &'a AppDirs,
    pub id: &'a str,
    /// The catalog entry, if it is still in the catalog (for `preserve` and
    /// `user_data_paths`).
    pub entry: Option<&'a CatalogEntry>,
    pub keep_saves: bool,
    /// Resolves `user_data_paths`; [`expand_user_path`] outside tests.
    pub expand: &'a dyn Fn(&str) -> Option<PathBuf>,
}

/// Remove an installed app. Blocking.
pub fn uninstall(req: UninstallRequest<'_>) -> Result<(InstalledApp, UninstallOutcome)> {
    let installed_file = req.dirs.installed_file();
    let db = InstalledDb::load(&installed_file)?;
    let app = db
        .get(req.id)
        .cloned()
        .ok_or_else(|| AppError::NotInstalled { id: req.id.into() })?;
    update::ensure_not_running(&app.dir)?;
    let preserve = match req.entry {
        Some(entry) => update::preserve_set(entry)?,
        None => GlobSet::empty(),
    };
    let lib = req.dirs.library_containing(&app.dir);
    let revision = stamp();

    // Step 1: take the folder out of place in one move.
    let trash = lib.staging.join(format!("{}-uninstall-{revision}", app.id));
    let had_folder = app.dir.is_dir();
    if had_folder {
        fsutil::rename_retry(&app.dir, &trash)?;
    }
    let put_back = || {
        if had_folder {
            let _ = fsutil::rename_retry(&trash, &app.dir);
        }
    };

    // Step 2: keep the user's files, or delete their data outside too.
    let mut outcome = UninstallOutcome {
        kept_saves: None,
        removed_user_data: Vec::new(),
    };
    let mut kept: Vec<String> = Vec::new();
    if req.keep_saves && had_folder {
        let saves = lib
            .backups
            .join(&app.id)
            .join(format!("{SAVES_PREFIX}{revision}"));
        let result = user_files(&trash, &app.files, &preserve)
            .and_then(|files| move_files(&files, &trash, &saves).map(|()| files));
        match result {
            Ok(files) if files.is_empty() => {}
            Ok(files) => {
                kept = files;
                outcome.kept_saves = Some(crate::paths::normalize(&saves));
            }
            Err(err) => {
                put_back();
                return Err(err);
            }
        }
    }

    // Step 3: forget the app. Until here everything can be undone.
    let saved = InstalledDb::load(&installed_file).and_then(|mut db| {
        db.apps.remove(&app.id);
        db.save(&installed_file)
    });
    if let Err(err) = saved {
        if let Some(saves) = &outcome.kept_saves {
            let _ = move_files(&kept, saves, &trash);
        }
        put_back();
        return Err(err);
    }

    // Step 4: clean up; failures here only leave garbage behind, which the
    // staging cleanup at the next start removes.
    if !req.keep_saves {
        for path in req
            .entry
            .map(|e| e.user_data_paths.as_slice())
            .unwrap_or_default()
        {
            let Some(dir) = (req.expand)(path) else {
                continue;
            };
            if dir.is_dir() {
                match std::fs::remove_dir_all(&dir) {
                    Ok(()) => outcome.removed_user_data.push(dir),
                    Err(err) => tracing::warn!(?dir, %err, "could not delete user data"),
                }
            }
        }
    }
    if let Some(previous) = &app.previous {
        let _ = std::fs::remove_dir_all(&previous.dir);
    }
    if had_folder {
        if let Err(err) = std::fs::remove_dir_all(&trash) {
            tracing::warn!(?trash, %err, "uninstall leftovers stay until the next start");
        }
    }
    fsutil::remove_empty_dirs(&lib.backups.join(&app.id), &lib.backups);
    tracing::info!(id = %app.id, keep_saves = req.keep_saves, ?outcome, "uninstalled");
    Ok((app, outcome))
}

/// After a fresh install: move saves kept by an earlier uninstall back into
/// the game folder. Files the new release ships win; leftovers stay in the
/// saves folder. Returns how many files came back. Blocking.
pub fn restore_kept_saves(lib: &LibraryDirs, id: &str, target: &Path) -> Result<usize> {
    let root = lib.backups.join(id);
    let Ok(entries) = std::fs::read_dir(&root) else {
        return Ok(0);
    };
    let Some(saves) = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_dir()
                && p.file_name()
                    .is_some_and(|n| n.to_string_lossy().starts_with(SAVES_PREFIX))
        })
        .max()
    else {
        return Ok(0);
    };
    let mut restored = 0;
    for rel in archive::list_files(&saves)? {
        let to = target.join(&rel);
        if to.exists() {
            tracing::warn!(%rel, "kept save not restored: the new release ships this file");
            continue;
        }
        fsutil::rename_retry(&saves.join(&rel), &to)?;
        restored += 1;
    }
    // Drop the folder if nothing is left in it.
    if archive::list_files(&saves)?.is_empty() {
        let _ = std::fs::remove_dir_all(&saves);
        fsutil::remove_empty_dirs(&root, &lib.backups);
    }
    Ok(restored)
}

#[cfg(test)]
mod tests;
