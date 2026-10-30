//! Cancellation and progress reporting for long-running jobs.

use crate::error::{AppError, Result};
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Shared flag; the job checks it between chunks and files.
#[derive(Debug, Clone, Default)]
pub struct CancelFlag(Arc<AtomicBool>);

impl CancelFlag {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }

    /// `Err(Cancelled)` once cancelled, for use with `?`.
    pub fn check(&self) -> Result<()> {
        if self.is_cancelled() {
            Err(AppError::Cancelled)
        } else {
            Ok(())
        }
    }
}

/// What a job is doing right now. Byte and file counts let the UI draw a bar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "phase", rename_all = "snake_case")]
pub enum Progress {
    Resolving,
    Downloading { done: u64, total: u64 },
    Verifying,
    Extracting { done: u64, total: u64 },
    Finalizing,
}

/// Progress sink. Called often; implementations should be cheap.
pub type ProgressFn<'a> = &'a (dyn Fn(Progress) + Send + Sync);
