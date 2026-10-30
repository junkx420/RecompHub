//! User settings, stored in `settings.json`.
//!
//! Unknown or missing fields fall back to defaults, so older and newer app
//! versions can share one file. The GitHub token is deliberately not stored
//! here; it goes to the OS credential store.

use crate::error::{AppError, Result};
use crate::fsutil;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const DEFAULT_CATALOG_URL: &str =
    "https://raw.githubusercontent.com/junkx420/ReCompX-Store/main/catalog/catalog.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    #[default]
    System,
    De,
    En,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub language: Language,
    pub theme: Theme,
    /// `None` means the default `%LOCALAPPDATA%\RecompHub\apps`.
    pub library_dir: Option<PathBuf>,
    pub catalog_url: String,
    pub check_updates: bool,
    pub desktop_shortcut: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            language: Language::System,
            theme: Theme::System,
            library_dir: None,
            catalog_url: DEFAULT_CATALOG_URL.to_owned(),
            check_updates: true,
            desktop_shortcut: false,
        }
    }
}

impl Settings {
    /// Load settings. A missing file yields defaults; a corrupt file is moved
    /// aside to `settings.json.broken` and defaults are used.
    pub fn load(path: &Path) -> Self {
        match std::fs::read(path) {
            Ok(bytes) => match serde_json::from_slice(&bytes) {
                Ok(settings) => settings,
                Err(err) => {
                    tracing::warn!(%err, "settings.json is invalid, using defaults");
                    let _ = std::fs::rename(path, path.with_extension("json.broken"));
                    Self::default()
                }
            },
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        fsutil::write_json(path, self)
    }

    pub fn validate(&self) -> Result<()> {
        let url = url::Url::parse(&self.catalog_url).map_err(|e| AppError::InvalidSettings {
            message: format!("catalog_url: {e}"),
        })?;
        if url.scheme() != "https" {
            return Err(AppError::InvalidSettings {
                message: "catalog_url must use https".into(),
            });
        }
        if let Some(dir) = &self.library_dir {
            if !dir.is_absolute() {
                return Err(AppError::InvalidSettings {
                    message: "library_dir must be an absolute path".into(),
                });
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_gives_defaults() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            Settings::load(&dir.path().join("nope.json")),
            Settings::default()
        );
    }

    #[test]
    fn roundtrip_and_partial_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, r#"{"language":"de","unknown_field":1}"#).unwrap();
        let settings = Settings::load(&path);
        assert_eq!(settings.language, Language::De);
        assert_eq!(settings.theme, Theme::System);

        let changed = Settings {
            theme: Theme::Dark,
            ..settings
        };
        changed.save(&path).unwrap();
        assert_eq!(Settings::load(&path), changed);
    }

    #[test]
    fn corrupt_file_is_moved_aside() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, b"{not json").unwrap();
        assert_eq!(Settings::load(&path), Settings::default());
        assert!(dir.path().join("settings.json.broken").exists());
    }

    #[test]
    fn validate_rejects_http_and_relative_library() {
        let s = Settings {
            catalog_url: "http://example.com/c.json".into(),
            ..Settings::default()
        };
        assert!(s.validate().is_err());
        let s = Settings {
            library_dir: Some("relative/dir".into()),
            ..Settings::default()
        };
        assert!(s.validate().is_err());
        assert!(Settings::default().validate().is_ok());
    }
}
