//! Starting games, with their console output captured to a log file.
//!
//! Each start truncates `logs\<id>.log`, writes a short header, and points
//! the game's stdout and stderr there. The caller awaits [`Launched::wait`]
//! to learn when the game exits; the exit code is appended to the log.

use crate::catalog::CatalogEntry;
use crate::error::{AppError, IoContext, Result};
use crate::gamedata;
use crate::installed::InstalledApp;
use crate::paths::{self, AppDirs};
use crate::process;
use chrono::Utc;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;

pub fn log_path(dirs: &AppDirs, id: &str) -> PathBuf {
    dirs.logs().join(format!("{id}.log"))
}

pub struct Launched {
    child: tokio::process::Child,
    pub log: PathBuf,
    pub pid: Option<u32>,
}

impl Launched {
    /// Wait for the game to exit and note the exit code in the log.
    pub async fn wait(mut self) -> Option<i32> {
        let code = match self.child.wait().await {
            Ok(status) => status.code(),
            Err(err) => {
                tracing::warn!(%err, "waiting for game failed");
                None
            }
        };
        if let Ok(mut log) = std::fs::OpenOptions::new().append(true).open(&self.log) {
            let code = code.map_or_else(|| "unknown".to_owned(), |c| c.to_string());
            let _ = writeln!(
                log,
                "\n# RecompHub: exited at {} with code {code}",
                Utc::now()
            );
        }
        code
    }
}

/// Start `app`. Fails if its game data is missing, the executable is gone,
/// or a program from its folder is already running.
pub fn launch(dirs: &AppDirs, app: &InstalledApp, entry: &CatalogEntry) -> Result<Launched> {
    gamedata::ready_to_launch(app, entry)?;
    let running = process::running_in(&app.dir);
    if !running.is_empty() {
        return Err(AppError::AlreadyRunning { processes: running });
    }
    // Working directories and process creation do not take `\\?\` paths.
    let dir = paths::normalize(&app.dir);
    let exe = paths::normalize(&dir.join(&app.exe));
    if !exe.is_file() {
        return Err(AppError::ExeNotFound {
            pattern: app.exe.clone(),
        });
    }

    let log = log_path(dirs, &app.id);
    std::fs::create_dir_all(dirs.logs()).at(dirs.logs())?;
    let mut out = std::fs::File::create(&log).at(&log)?;
    writeln!(
        out,
        "# RecompHub: starting {} {} at {}\n# {}\n",
        app.name,
        app.version,
        Utc::now(),
        exe.display()
    )
    .at(&log)?;
    let err = out.try_clone().at(&log)?;

    let mut command = tokio::process::Command::new(&exe);
    command
        .current_dir(&dir)
        .stdin(Stdio::null())
        .stdout(out)
        .stderr(err)
        .kill_on_drop(false);
    hide_console(&mut command);
    let child = command.spawn().at(&exe)?;
    let pid = child.id();
    tracing::info!(id = %app.id, ?pid, exe = ?exe, "game started");
    Ok(Launched { child, log, pid })
}

/// Console-subsystem ports would otherwise open an empty console window;
/// their output goes to the log instead. GUI windows are unaffected.
#[cfg(windows)]
fn hide_console(command: &mut tokio::process::Command) {
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn hide_console(_: &mut tokio::process::Command) {}

/// Last `max_bytes` of a log, for showing in the UI.
pub fn log_tail(path: &Path, max_bytes: u64) -> Result<String> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = std::fs::File::open(path).at(path)?;
    let len = file.metadata().at(path)?.len();
    file.seek(SeekFrom::Start(len.saturating_sub(max_bytes)))
        .at(path)?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).at(path)?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::GameData;
    use std::collections::BTreeMap;

    fn app_in(dir: &Path, exe: &str) -> (InstalledApp, CatalogEntry) {
        let (entries, _) = crate::catalog::parse(
            crate::catalog::EMBEDDED_CATALOG.as_bytes(),
            Default::default(),
        )
        .unwrap();
        let mut entry = entries.into_iter().next().unwrap();
        entry.game_data = GameData::None;
        let app = InstalledApp {
            id: "test-game".into(),
            name: "Test".into(),
            version: "v1".into(),
            asset_name: "a.zip".into(),
            asset_id: 1,
            asset_sha256: None,
            dir: dir.to_path_buf(),
            exe: exe.into(),
            installed_at: Utc::now(),
            updated_at: None,
            files: BTreeMap::new(),
            previous: None,
            revision: "r".into(),
            shortcuts: Vec::new(),
            steam_users: Vec::new(),
        };
        (app, entry)
    }

    #[test]
    fn missing_exe_is_reported() {
        let dir = tempfile::tempdir().unwrap();
        let dirs = AppDirs::at(dir.path().join("hub"));
        let (app, entry) = app_in(&dir.path().join("game"), "nope.exe");
        std::fs::create_dir_all(&app.dir).unwrap();
        assert!(matches!(
            launch(&dirs, &app, &entry),
            Err(AppError::ExeNotFound { .. })
        ));
    }

    #[test]
    fn log_tail_returns_the_end() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.log");
        std::fs::write(&path, "first line\nlast line\n").unwrap();
        assert_eq!(log_tail(&path, 10).unwrap(), "last line\n");
        assert_eq!(log_tail(&path, 1000).unwrap(), "first line\nlast line\n");
    }

    /// Uses a copy of `whoami.exe` as the "game": it prints and exits.
    #[cfg(windows)]
    #[tokio::test]
    async fn captures_output_and_exit_code() {
        let dir = tempfile::tempdir().unwrap();
        let dirs = AppDirs::at(dir.path().join("hub"));
        let game = dir.path().join("game");
        std::fs::create_dir_all(&game).unwrap();
        let system = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
        std::fs::copy(
            Path::new(&system).join(r"System32\whoami.exe"),
            game.join("game.exe"),
        )
        .unwrap();
        let (app, entry) = app_in(&game, "game.exe");

        let launched = launch(&dirs, &app, &entry).unwrap();
        let log = launched.log.clone();
        assert_eq!(launched.wait().await, Some(0));
        let text = std::fs::read_to_string(log).unwrap();
        assert!(text.starts_with("# RecompHub: starting Test v1"), "{text}");
        let username = std::env::var("USERNAME").unwrap().to_lowercase();
        assert!(text.to_lowercase().contains(&username), "{text}");
        assert!(text.contains("with code 0"), "{text}");
    }
}
