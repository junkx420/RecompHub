//! RecompHub core: everything that does not need Tauri.
//!
//! The Tauri crate is a thin command layer over this crate, so all logic can
//! be tested on any OS against temporary directories and a mock HTTP server.

pub mod archive;
pub mod catalog;
pub mod download;
pub mod error;
pub mod fsutil;
pub mod gamedata;
pub mod github;
pub mod http;
pub mod install;
pub mod installed;
pub mod job;
pub mod launch;
pub mod paths;
pub mod platform;
pub mod process;
pub mod settings;
pub mod update;

pub use error::{AppError, Result};
