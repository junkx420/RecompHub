//! RecompHub core: everything that does not need Tauri.
//!
//! The Tauri crate is a thin command layer over this crate, so all logic can
//! be tested on any OS against temporary directories and a mock HTTP server.

pub mod catalog;
pub mod error;
pub mod fsutil;
pub mod github;
pub mod http;
pub mod paths;
pub mod platform;
pub mod settings;

pub use error::{AppError, Result};
