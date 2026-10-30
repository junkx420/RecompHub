use super::Platform;
use crate::error::{AppError, Result};
use std::path::PathBuf;

/// Minimal non-Windows backend for development and tests.
pub struct StubPlatform;

impl Platform for StubPlatform {
    fn local_data_dir(&self) -> Result<PathBuf> {
        if let Some(dir) = std::env::var_os("XDG_DATA_HOME") {
            return Ok(PathBuf::from(dir));
        }
        std::env::var_os("HOME")
            .map(|home| PathBuf::from(home).join(".local").join("share"))
            .ok_or(AppError::NotFound {
                what: "$HOME".into(),
            })
    }
}
