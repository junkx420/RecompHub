//! `installed.json`: what RecompHub installed, where, and which files.
//!
//! The file hashes let later updates tell files that came with the port
//! (safe to replace) from files the port or the user created (always kept).

use crate::error::{AppError, Result};
use crate::fsutil;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstalledApp {
    pub id: String,
    pub name: String,
    /// Release tag, e.g. `v1.2.0`.
    pub version: String,
    pub asset_name: String,
    pub asset_id: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset_sha256: Option<String>,
    /// Absolute game folder.
    pub dir: PathBuf,
    /// Executable, `/`-separated and relative to `dir`.
    pub exe: String,
    pub installed_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<DateTime<Utc>>,
    /// Relative path (`/`-separated) to hex SHA-256, for every file this
    /// release placed. Anything else in `dir` belongs to the port or user.
    pub files: BTreeMap<String, String>,
    /// Backup of the version before the last update or restore.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous: Option<PreviousVersion>,
    /// Changes with every install, update and restore; an interrupted
    /// update compares it with its journal to tell commit from rollback.
    #[serde(default)]
    pub revision: String,
    /// `.lnk` files RecompHub created for this app.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shortcuts: Vec<PathBuf>,
    /// Steam account ids (32-bit) this app was added to as a shortcut.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub steam_users: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreviousVersion {
    pub version: String,
    /// Backup folder (holds `backup.json` and `files/`).
    pub dir: PathBuf,
}

/// What the UI needs per installed app (no file list).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InstalledSummary {
    pub id: String,
    pub name: String,
    pub version: String,
    pub dir: PathBuf,
    pub exe: String,
    pub installed_at: DateTime<Utc>,
    pub updated_at: Option<DateTime<Utc>>,
    pub file_count: usize,
    pub previous_version: Option<String>,
    pub shortcuts: Vec<PathBuf>,
    pub steam_users: Vec<u32>,
}

impl InstalledApp {
    pub fn summary(&self) -> InstalledSummary {
        InstalledSummary {
            id: self.id.clone(),
            name: self.name.clone(),
            version: self.version.clone(),
            dir: crate::paths::normalize(&self.dir),
            exe: self.exe.clone(),
            installed_at: self.installed_at,
            updated_at: self.updated_at,
            file_count: self.files.len(),
            previous_version: self.previous.as_ref().map(|p| p.version.clone()),
            shortcuts: self
                .shortcuts
                .iter()
                .map(|p| crate::paths::normalize(p))
                .collect(),
            steam_users: self.steam_users.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstalledDb {
    pub schema_version: u32,
    #[serde(default)]
    pub apps: BTreeMap<String, InstalledApp>,
}

impl Default for InstalledDb {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            apps: BTreeMap::new(),
        }
    }
}

impl InstalledDb {
    /// Load the database. A missing file is an empty database. A corrupt
    /// file is moved aside to `installed.json.broken` (never deleted) and an
    /// empty database is returned, so the app stays usable.
    pub fn load(path: &Path) -> Result<Self> {
        let bytes = match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(err) => return Err(AppError::io(path, err)),
        };
        match serde_json::from_slice::<Self>(&bytes) {
            Ok(db) if db.schema_version <= SCHEMA_VERSION => Ok(db),
            Ok(db) => Err(AppError::InvalidInput {
                message: format!(
                    "installed.json schema_version {} is newer than supported {SCHEMA_VERSION}; update RecompHub",
                    db.schema_version
                ),
            }),
            Err(err) => {
                tracing::error!(%err, "installed.json is invalid, moving it aside");
                let broken = path.with_extension("json.broken");
                std::fs::rename(path, &broken).map_err(|e| AppError::io(&broken, e))?;
                Ok(Self::default())
            }
        }
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        fsutil::write_json(path, self)
    }

    pub fn get(&self, id: &str) -> Option<&InstalledApp> {
        self.apps.get(id)
    }

    /// Load, change one app's record with `f`, and save.
    pub fn modify<T>(
        path: &Path,
        id: &str,
        f: impl FnOnce(&mut InstalledApp) -> Result<T>,
    ) -> Result<(T, InstalledApp)> {
        let mut db = Self::load(path)?;
        let app = db
            .apps
            .get_mut(id)
            .ok_or_else(|| AppError::NotInstalled { id: id.into() })?;
        let value = f(app)?;
        let app = app.clone();
        db.save(path)?;
        Ok((value, app))
    }

    pub fn summaries(&self) -> Vec<InstalledSummary> {
        let mut list: Vec<_> = self.apps.values().map(InstalledApp::summary).collect();
        list.sort_by_key(|a| a.name.to_lowercase());
        list
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(id: &str) -> InstalledApp {
        InstalledApp {
            id: id.into(),
            name: id.to_uppercase(),
            version: "v1".into(),
            asset_name: "a.zip".into(),
            asset_id: 1,
            asset_sha256: None,
            dir: PathBuf::from("/games").join(id),
            exe: "game.exe".into(),
            installed_at: Utc::now(),
            updated_at: None,
            files: BTreeMap::from([("game.exe".into(), "00".into())]),
            previous: None,
            revision: "r1".into(),
            shortcuts: Vec::new(),
            steam_users: Vec::new(),
        }
    }

    #[test]
    fn roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("installed.json");
        assert_eq!(InstalledDb::load(&path).unwrap(), InstalledDb::default());
        let mut db = InstalledDb::default();
        db.apps.insert("b".into(), app("b"));
        db.apps.insert("a".into(), app("a"));
        db.save(&path).unwrap();
        let loaded = InstalledDb::load(&path).unwrap();
        assert_eq!(loaded, db);
        let names: Vec<_> = loaded.summaries().into_iter().map(|s| s.name).collect();
        assert_eq!(names, ["A", "B"]);
    }

    #[test]
    fn corrupt_file_is_kept_aside() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("installed.json");
        std::fs::write(&path, b"{oops").unwrap();
        assert_eq!(InstalledDb::load(&path).unwrap(), InstalledDb::default());
        assert_eq!(
            std::fs::read(dir.path().join("installed.json.broken")).unwrap(),
            b"{oops"
        );
    }

    #[test]
    fn newer_schema_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("installed.json");
        std::fs::write(&path, br#"{"schema_version": 99, "apps": {}}"#).unwrap();
        assert!(InstalledDb::load(&path).is_err());
        assert!(path.exists());
    }
}
