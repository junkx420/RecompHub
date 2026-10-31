use super::{Platform, ShortcutSpec, UserFolder};
use crate::error::{AppError, Result};
use std::path::{Path, PathBuf};
use windows::core::{Interface, GUID, HSTRING};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, IPersistFile,
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
};
use windows::Win32::UI::Shell::{
    FOLDERID_Desktop, FOLDERID_LocalAppData, FOLDERID_Programs, IShellLinkW, SHGetKnownFolderPath,
    ShellLink, KF_FLAG_DEFAULT,
};

pub struct WindowsPlatform;

fn known_folder(id: &GUID, name: &str) -> Result<PathBuf> {
    // SAFETY: SHGetKnownFolderPath returns a CoTaskMem-allocated wide string
    // that we copy and free exactly once.
    unsafe {
        let pwstr =
            SHGetKnownFolderPath(id, KF_FLAG_DEFAULT, None).map_err(|e| AppError::NotFound {
                what: format!("{name}: {e}"),
            })?;
        let path = pwstr.to_string();
        CoTaskMemFree(Some(pwstr.0 as *const _));
        path.map(PathBuf::from).map_err(|e| AppError::InvalidInput {
            message: format!("{name} path is not valid UTF-16: {e}"),
        })
    }
}

fn com_error(what: &str, err: windows::core::Error) -> AppError {
    AppError::InvalidInput {
        message: format!("{what}: {err}"),
    }
}

/// COM for the current thread; uninitialized on drop if we initialized it.
struct ComGuard(bool);

impl ComGuard {
    fn new() -> Self {
        // SAFETY: plain COM initialization. S_FALSE (already initialized)
        // counts as success and must be balanced too; RPC_E_CHANGED_MODE
        // (the thread is MTA) is an error we can work with as is.
        let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        Self(hr.is_ok())
    }
}

impl Drop for ComGuard {
    fn drop(&mut self) {
        if self.0 {
            // SAFETY: balances the successful CoInitializeEx above.
            unsafe { CoUninitialize() };
        }
    }
}

impl Platform for WindowsPlatform {
    fn local_data_dir(&self) -> Result<PathBuf> {
        known_folder(&FOLDERID_LocalAppData, "LocalAppData").or_else(|_| {
            std::env::var_os("LOCALAPPDATA")
                .map(PathBuf::from)
                .ok_or(AppError::NotFound {
                    what: "%LOCALAPPDATA%".into(),
                })
        })
    }

    fn user_folder(&self, folder: UserFolder) -> Result<PathBuf> {
        match folder {
            UserFolder::StartMenuPrograms => known_folder(&FOLDERID_Programs, "Programs"),
            UserFolder::Desktop => known_folder(&FOLDERID_Desktop, "Desktop"),
        }
    }

    fn create_shortcut(&self, path: &Path, spec: &ShortcutSpec<'_>) -> Result<()> {
        let _com = ComGuard::new();
        // SAFETY: standard IShellLinkW usage; all strings outlive the calls.
        unsafe {
            let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)
                .map_err(|e| com_error("ShellLink", e))?;
            link.SetPath(&HSTRING::from(spec.target.as_os_str()))
                .map_err(|e| com_error("SetPath", e))?;
            link.SetWorkingDirectory(&HSTRING::from(spec.working_dir.as_os_str()))
                .map_err(|e| com_error("SetWorkingDirectory", e))?;
            link.SetDescription(&HSTRING::from(spec.description))
                .map_err(|e| com_error("SetDescription", e))?;
            link.SetIconLocation(&HSTRING::from(spec.target.as_os_str()), 0)
                .map_err(|e| com_error("SetIconLocation", e))?;
            let file: IPersistFile = link.cast().map_err(|e| com_error("IPersistFile", e))?;
            file.Save(&HSTRING::from(path.as_os_str()), true)
                .map_err(|e| com_error("save shortcut", e))
        }
    }

    fn steam_dir(&self) -> Option<PathBuf> {
        use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_SZ};
        let mut buf = [0u16; 1024];
        let mut size = (buf.len() * 2) as u32;
        // SAFETY: buffer and size describe the same writable memory.
        let status = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                &HSTRING::from(r"Software\Valve\Steam"),
                &HSTRING::from("SteamPath"),
                RRF_RT_REG_SZ,
                None,
                Some(buf.as_mut_ptr().cast()),
                Some(&mut size),
            )
        };
        if status.is_err() {
            return None;
        }
        let len = (size as usize / 2).saturating_sub(1); // drop the NUL
        let path = PathBuf::from(String::from_utf16_lossy(&buf[..len]).replace('/', "\\"));
        path.is_dir().then_some(path)
    }
}
