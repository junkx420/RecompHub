//! OS-specific functionality behind a trait.
//!
//! Only Windows is a supported target. `stub` exists so the core compiles and
//! its tests run on other systems; a real Linux backend can replace it later.

use crate::error::Result;
use std::path::PathBuf;

#[cfg(not(windows))]
mod stub;
#[cfg(windows)]
mod windows;

pub trait Platform: Send + Sync {
    /// Per-user local data directory, e.g. `%LOCALAPPDATA%`.
    fn local_data_dir(&self) -> Result<PathBuf>;
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
