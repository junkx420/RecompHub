//! Start menu and desktop shortcuts.
//!
//! Start menu links go into a `RecompHub` folder under the user's
//! Programs folder; desktop links sit on the desktop directly. Every link
//! RecompHub creates is recorded in `InstalledApp::shortcuts`, so it can
//! be refreshed after an update and removed later.

use crate::download::sanitize_file_name;
use crate::error::{IoContext, Result};
use crate::installed::InstalledApp;
use crate::paths;
use crate::platform::{Platform, ShortcutSpec, UserFolder};
use std::path::{Path, PathBuf};

const START_MENU_FOLDER: &str = "RecompHub";

fn link_name(app: &InstalledApp) -> String {
    format!("{}.lnk", sanitize_file_name(&app.name))
}

/// Where the links for `app` go.
pub fn link_paths(
    platform: &dyn Platform,
    app: &InstalledApp,
    desktop: bool,
) -> Result<Vec<PathBuf>> {
    let mut paths = vec![platform
        .user_folder(UserFolder::StartMenuPrograms)?
        .join(START_MENU_FOLDER)
        .join(link_name(app))];
    if desktop {
        paths.push(
            platform
                .user_folder(UserFolder::Desktop)?
                .join(link_name(app)),
        );
    }
    Ok(paths)
}

/// Write links at `paths` pointing at the app's current executable.
pub fn write(platform: &dyn Platform, app: &InstalledApp, paths: &[PathBuf]) -> Result<()> {
    let dir = paths::normalize(&app.dir);
    let target = paths::normalize(&dir.join(&app.exe));
    let spec = ShortcutSpec {
        target: &target,
        working_dir: &dir,
        description: &app.name,
    };
    for path in paths {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).at(parent)?;
        }
        platform.create_shortcut(path, &spec)?;
    }
    Ok(())
}

/// Delete links (missing ones are fine) and the start menu folder once empty.
pub fn remove(paths: &[PathBuf]) {
    for path in paths {
        match std::fs::remove_file(path) {
            Ok(()) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => tracing::warn!(?path, %err, "could not remove shortcut"),
        }
        if let Some(parent) = path.parent() {
            if parent.file_name().is_some_and(|n| n == START_MENU_FOLDER) {
                let _ = std::fs::remove_dir(parent);
            }
        }
    }
}

/// Does `path` look like one of our links (safe to delete)?
pub fn is_link(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("lnk"))
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    struct TempFolders(tempfile::TempDir);

    impl Platform for TempFolders {
        fn local_data_dir(&self) -> Result<PathBuf> {
            Ok(self.0.path().to_path_buf())
        }
        fn user_folder(&self, folder: UserFolder) -> Result<PathBuf> {
            Ok(self.0.path().join(format!("{folder:?}")))
        }
        fn create_shortcut(&self, path: &Path, spec: &ShortcutSpec<'_>) -> Result<()> {
            crate::platform::current().create_shortcut(path, spec)
        }
        fn steam_dir(&self) -> Option<PathBuf> {
            None
        }
    }

    #[test]
    fn creates_real_lnk_files_and_removes_them() {
        let folders = TempFolders(tempfile::tempdir().unwrap());
        let game = folders.0.path().join("game");
        std::fs::create_dir_all(&game).unwrap();
        std::fs::write(game.join("game.exe"), b"MZ").unwrap();
        let app = InstalledApp {
            id: "x".into(),
            name: "Zelda 64: Recompiled".into(),
            version: "v1".into(),
            asset_name: "a.zip".into(),
            asset_id: 1,
            asset_sha256: None,
            dir: game,
            exe: "game.exe".into(),
            installed_at: chrono::Utc::now(),
            updated_at: None,
            files: Default::default(),
            previous: None,
            revision: "r".into(),
            shortcuts: Vec::new(),
            steam_users: Vec::new(),
        };

        let paths = link_paths(&folders, &app, true).unwrap();
        assert_eq!(paths.len(), 2);
        assert!(
            paths[0].ends_with(r"RecompHub\Zelda 64_ Recompiled.lnk"),
            "{paths:?}"
        );
        write(&folders, &app, &paths).unwrap();
        for path in &paths {
            let bytes = std::fs::read(path).unwrap();
            // Shell link header: size 0x4C and the ShellLink CLSID.
            assert_eq!(&bytes[..4], &[0x4C, 0, 0, 0]);
            assert!(is_link(path));
        }

        remove(&paths);
        assert!(paths.iter().all(|p| !p.exists()));
        assert!(
            !paths[0].parent().unwrap().exists(),
            "empty RecompHub folder removed"
        );
    }
}
