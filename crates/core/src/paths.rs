//! Directory layout.
//!
//! ```text
//! %LOCALAPPDATA%\RecompHub\
//!   settings.json  installed.json
//!   cache\  logs\  staging\  backups\  apps\<id>\
//! ```
//!
//! With a custom library folder (e.g. `D:\Games\Recomps`), staging and backups
//! move to `<library>\.recomphub\` so that swapping game folders is a rename on
//! one volume instead of a slow, non-atomic cross-volume copy.
//!
//! Long paths: Rust's std adds the `\\?\` prefix itself for file system calls
//! on Windows. Use [`normalize`] before handing paths to APIs that do not
//! (process working directory, shell links) or before showing them to users.

use crate::error::{IoContext, Result};
use crate::platform::Platform;
use std::path::{Path, PathBuf};

pub const APP_DIR_NAME: &str = "RecompHub";
const LIBRARY_META_DIR: &str = ".recomphub";

#[derive(Debug, Clone)]
pub struct AppDirs {
    root: PathBuf,
}

impl AppDirs {
    pub fn from_platform(platform: &dyn Platform) -> Result<Self> {
        Ok(Self::at(platform.local_data_dir()?.join(APP_DIR_NAME)))
    }

    /// Layout rooted at an explicit directory (used by tests).
    pub fn at(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn settings_file(&self) -> PathBuf {
        self.root.join("settings.json")
    }
    pub fn installed_file(&self) -> PathBuf {
        self.root.join("installed.json")
    }
    pub fn cache(&self) -> PathBuf {
        self.root.join("cache")
    }
    pub fn logs(&self) -> PathBuf {
        self.root.join("logs")
    }
    pub fn default_library(&self) -> PathBuf {
        self.root.join("apps")
    }

    /// Resolve the library and its staging/backup folders.
    pub fn library(&self, custom: Option<&Path>) -> LibraryDirs {
        match custom {
            Some(dir) => LibraryDirs {
                apps: dir.to_path_buf(),
                staging: dir.join(LIBRARY_META_DIR).join("staging"),
                backups: dir.join(LIBRARY_META_DIR).join("backups"),
            },
            None => LibraryDirs {
                apps: self.default_library(),
                staging: self.root.join("staging"),
                backups: self.root.join("backups"),
            },
        }
    }

    /// The library an installed game folder belongs to. The library setting
    /// may have changed since the install, so updates use the folder's own
    /// library to keep staging and backups on its volume.
    pub fn library_containing(&self, app_dir: &Path) -> LibraryDirs {
        match app_dir.parent() {
            Some(parent) if parent != self.default_library() => self.library(Some(parent)),
            _ => self.library(None),
        }
    }

    /// Create the fixed app directories.
    pub fn ensure(&self) -> Result<()> {
        for dir in [self.root.clone(), self.cache(), self.logs()] {
            std::fs::create_dir_all(&dir).at(&dir)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryDirs {
    pub apps: PathBuf,
    pub staging: PathBuf,
    pub backups: PathBuf,
}

impl LibraryDirs {
    pub fn app(&self, id: &str) -> PathBuf {
        self.apps.join(id)
    }
}

/// Strip `\\?\` where possible so paths work with APIs that reject it and
/// read well in the UI. No-op on other platforms.
pub fn normalize(path: &Path) -> PathBuf {
    dunce::simplified(path).to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_library_lives_under_root() {
        let dirs = AppDirs::at("/data/RecompHub");
        let lib = dirs.library(None);
        assert_eq!(lib.apps, Path::new("/data/RecompHub/apps"));
        assert_eq!(lib.staging, Path::new("/data/RecompHub/staging"));
        assert_eq!(
            lib.app("zelda64recomp"),
            Path::new("/data/RecompHub/apps/zelda64recomp")
        );
    }

    #[test]
    fn custom_library_keeps_staging_on_same_volume() {
        let dirs = AppDirs::at("/data/RecompHub");
        let lib = dirs.library(Some(Path::new("/games/recomps")));
        assert_eq!(lib.apps, Path::new("/games/recomps"));
        assert!(lib.staging.starts_with("/games/recomps"));
        assert!(lib.backups.starts_with("/games/recomps"));
    }
}
