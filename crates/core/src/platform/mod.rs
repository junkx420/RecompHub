//! OS-specific functionality behind a trait.
//!
//! Only Windows is a supported target. `stub` exists so the core compiles and
//! its tests run on other systems; a real Linux backend can replace it later.

use crate::error::Result;
use std::path::{Path, PathBuf};

#[cfg(not(windows))]
mod stub;
#[cfg(windows)]
mod windows;

/// Per-user folders shortcuts go into.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserFolder {
    /// `%APPDATA%\Microsoft\Windows\Start Menu\Programs`
    StartMenuPrograms,
    Desktop,
}

/// A Windows shell link (`.lnk`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShortcutSpec<'a> {
    pub target: &'a Path,
    pub working_dir: &'a Path,
    pub description: &'a str,
}

pub trait Platform: Send + Sync {
    /// Per-user local data directory, e.g. `%LOCALAPPDATA%`.
    fn local_data_dir(&self) -> Result<PathBuf>;

    fn user_folder(&self, folder: UserFolder) -> Result<PathBuf>;

    /// Write a shell link to `path` (overwriting it).
    fn create_shortcut(&self, path: &Path, spec: &ShortcutSpec<'_>) -> Result<()>;

    /// Steam's install folder, if Steam is installed for this user.
    fn steam_dir(&self) -> Option<PathBuf>;
}

/// The backend for the current OS.
pub fn current() -> &'static dyn Platform {
    #[cfg(windows)]
    {
        &windows::WindowsPlatform
    }
    #[cfg(not(windows))]
    {
        &stub::StubPlatform
    }
}
