//! Error type shared by the core and the Tauri layer.
//!
//! Every variant serializes to `{ "code": "...", "params": { ... } }`. The
//! frontend translates `code` via i18n (`errors.<code>`) and interpolates
//! `params`, so user-facing text never lives in Rust.

use serde::Serialize;
use std::path::PathBuf;

pub type Result<T, E = AppError> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error, Serialize)]
#[serde(tag = "code", content = "params", rename_all = "snake_case")]
pub enum AppError {
    #[error("network error: {message}")]
    Network { message: String },

    #[error("GitHub rate limit reached, resets at {reset_at}")]
    RateLimited {
        /// Unix timestamp (seconds) when the limit resets.
        reset_at: i64,
    },

    #[error("not found: {what}")]
    NotFound { what: String },

    #[error("HTTP {status} from {url}")]
    Http { status: u16, url: String },

    #[error("invalid catalog: {errors:?}")]
    CatalogInvalid { errors: Vec<String> },

    #[error("unknown app id {id}")]
    UnknownApp { id: String },

    #[error("no asset matches {pattern}")]
    AssetNoMatch {
        pattern: String,
        candidates: Vec<String>,
    },

    #[error("several assets match {pattern}: {matches:?}")]
    AssetAmbiguous {
        pattern: String,
        matches: Vec<String>,
    },

    #[error("checksum mismatch: expected {expected}, got {actual}")]
    ChecksumMismatch { expected: String, actual: String },

    #[error("I/O error at {path:?}: {message}")]
    Io { path: PathBuf, message: String },

    #[error("invalid settings: {message}")]
    InvalidSettings { message: String },

    #[error("invalid input: {message}")]
    InvalidInput { message: String },

    #[error("download size mismatch: expected {expected} bytes, got {actual}")]
    SizeMismatch { expected: u64, actual: u64 },

    #[error("unsupported archive {name}")]
    UnsupportedArchive { name: String },

    #[error("archive entry escapes the target folder: {entry}")]
    UnsafeArchivePath { entry: String },

    #[error("invalid archive: {message}")]
    ArchiveInvalid { message: String },

    #[error("no file matches the executable pattern {pattern}")]
    ExeNotFound { pattern: String },

    #[error("{path:?} already contains files that RecompHub did not install")]
    ForeignContent { path: PathBuf },

    #[error("{id} is already installed")]
    AlreadyInstalled { id: String },

    #[error("{id} is busy with another job")]
    Busy { id: String },

    #[error("{id} is not installed")]
    NotInstalled { id: String },

    #[error("{id} is already at {version}")]
    UpToDate { id: String, version: String },

    #[error("{id} has no previous version to restore")]
    NoPreviousVersion { id: String },

    #[error("close the game first: {processes:?} is running")]
    GameRunning { processes: Vec<String> },

    #[error("cancelled")]
    Cancelled,

    #[error("unsupported on this platform")]
    Unsupported,
}

impl AppError {
    pub fn io(path: impl Into<PathBuf>, err: std::io::Error) -> Self {
        AppError::Io {
            path: path.into(),
            message: err.to_string(),
        }
    }

    pub fn network(err: impl std::fmt::Display) -> Self {
        AppError::Network {
            message: err.to_string(),
        }
    }
}

impl From<reqwest::Error> for AppError {
    fn from(err: reqwest::Error) -> Self {
        match (err.status(), err.url()) {
            (Some(status), Some(url)) => AppError::Http {
                status: status.as_u16(),
                url: url.to_string(),
            },
            _ => AppError::network(err),
        }
    }
}

/// Extension to attach a path to `std::io::Error` results.
pub trait IoContext<T> {
    fn at(self, path: impl Into<PathBuf>) -> Result<T>;
}

impl<T> IoContext<T> for std::result::Result<T, std::io::Error> {
    fn at(self, path: impl Into<PathBuf>) -> Result<T> {
        self.map_err(|e| AppError::io(path, e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_as_code_and_params() {
        let json = serde_json::to_value(AppError::RateLimited { reset_at: 42 }).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"code": "rate_limited", "params": {"reset_at": 42}})
        );
        let json = serde_json::to_value(AppError::Cancelled).unwrap();
        assert_eq!(json, serde_json::json!({"code": "cancelled"}));
    }
}
