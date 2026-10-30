use super::Platform;
use crate::error::{AppError, Result};
use std::path::PathBuf;

pub struct WindowsPlatform;

impl Platform for WindowsPlatform {
    fn local_data_dir(&self) -> Result<PathBuf> {
        use windows::Win32::System::Com::CoTaskMemFree;
        use windows::Win32::UI::Shell::{
            FOLDERID_LocalAppData, SHGetKnownFolderPath, KF_FLAG_DEFAULT,
        };

        // SAFETY: SHGetKnownFolderPath returns a CoTaskMem-allocated wide string
        // that we copy and free exactly once.
        unsafe {
            match SHGetKnownFolderPath(&FOLDERID_LocalAppData, KF_FLAG_DEFAULT, None) {
                Ok(pwstr) => {
                    let path = pwstr.to_string();
                    CoTaskMemFree(Some(pwstr.0 as *const _));
                    path.map(PathBuf::from).map_err(|e| AppError::InvalidInput {
                        message: format!("LocalAppData path is not valid UTF-16: {e}"),
                    })
                }
                Err(_) => {
                    std::env::var_os("LOCALAPPDATA")
                        .map(PathBuf::from)
                        .ok_or(AppError::NotFound {
                            what: "%LOCALAPPDATA%".into(),
                        })
                }
            }
        }
    }
}
