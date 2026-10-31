//! Adding games to Steam as non-Steam shortcuts.
//!
//! Each Steam account has `userdata\<account id>\config\shortcuts.vdf`
//! (binary VDF). Steam rewrites that file on exit, so it must be closed
//! while we change it; the caller checks that with [`ensure_closed`].
//!
//! Our entries are recognized by their start folder (the game folder),
//! which survives updates even if the executable name changes, so adding
//! twice updates the entry instead of duplicating it. Before every write
//! the file is copied to `shortcuts.vdf.recomphub-<time>.bak`; the newest
//! few backups are kept.

use crate::error::{AppError, IoContext, Result};
use crate::fsutil;
use crate::installed::InstalledApp;
use crate::paths;
use crate::process;
use crate::vdf::{self, Map, Value};
use serde::Serialize;
use std::path::{Path, PathBuf};

/// Steam ids are this offset plus the 32-bit account id.
const STEAM_ID_BASE: u64 = 76_561_197_960_265_728;
const BACKUPS_KEPT: usize = 3;
const BACKUP_PREFIX: &str = "shortcuts.vdf.recomphub-";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SteamUser {
    pub account_id: u32,
    pub account_name: String,
    pub persona_name: String,
    /// The account Steam logged in with last; the UI preselects it.
    pub most_recent: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SteamStatus {
    pub found: bool,
    pub running: bool,
    pub users: Vec<SteamUser>,
}

pub fn ensure_closed() -> Result<()> {
    if process::is_running_named("steam.exe") {
        Err(AppError::SteamRunning)
    } else {
        Ok(())
    }
}

/// Accounts that have logged in on this machine, most recent first.
pub fn users(steam_dir: &Path) -> Result<Vec<SteamUser>> {
    let path = steam_dir.join("config").join("loginusers.vdf");
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => return Err(AppError::io(&path, err)),
    };
    let doc = vdf::parse_text(&text)?;
    let mut users: Vec<(i64, SteamUser)> = doc
        .get("users")
        .map(|u| u.entries())
        .unwrap_or_default()
        .iter()
        .filter_map(|(id, fields)| {
            let steam_id: u64 = id.parse().ok()?;
            let account_id = u32::try_from(steam_id.checked_sub(STEAM_ID_BASE)?).ok()?;
            let field = |k: &str| {
                fields
                    .get(k)
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_owned()
            };
            let timestamp = field("Timestamp").parse().unwrap_or(0);
            Some((
                timestamp,
                SteamUser {
                    account_id,
                    account_name: field("AccountName"),
                    persona_name: field("PersonaName"),
                    most_recent: field("MostRecent") == "1",
                },
            ))
        })
        .collect();
    users.sort_by(|(ta, a), (tb, b)| b.most_recent.cmp(&a.most_recent).then(tb.cmp(ta)));
    Ok(users.into_iter().map(|(_, u)| u).collect())
}

pub fn status(steam_dir: Option<&Path>) -> SteamStatus {
    let Some(dir) = steam_dir else {
        return SteamStatus {
            found: false,
            running: false,
            users: Vec::new(),
        };
    };
    SteamStatus {
        found: true,
        running: process::is_running_named("steam.exe"),
        users: users(dir).unwrap_or_else(|err| {
            tracing::warn!(%err, "cannot read Steam accounts");
            Vec::new()
        }),
    }
}

fn shortcuts_file(steam_dir: &Path, account_id: u32) -> PathBuf {
    steam_dir
        .join("userdata")
        .join(account_id.to_string())
        .join("config")
        .join("shortcuts.vdf")
}

/// Steam's id for a non-Steam shortcut: CRC-32 of exe and name, top bit set.
fn shortcut_appid(exe: &str, name: &str) -> u32 {
    crc32(format!("{exe}{name}").as_bytes()) | 0x8000_0000
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

fn quoted(path: &Path) -> String {
    format!("\"{}\"", paths::normalize(path).display())
}

/// Folder comparison as Steam stores it: quoted, any case, either slash.
fn same_dir(stored: &str, dir: &Path) -> bool {
    let clean = |s: &str| {
        s.trim_matches('"')
            .replace('/', "\\")
            .trim_end_matches('\\')
            .to_lowercase()
    };
    clean(stored) == clean(&paths::normalize(dir).display().to_string())
}

fn load(path: &Path) -> Result<Map> {
    match std::fs::read(path) {
        Ok(bytes) => {
            let doc = vdf::parse(&bytes)?;
            if doc.map("shortcuts").is_none() {
                return Err(AppError::InvalidInput {
                    message: format!("{} has no shortcuts list", path.display()),
                });
            }
            Ok(doc)
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            Ok(Map(vec![("shortcuts".into(), Value::Map(Map::default()))]))
        }
        Err(err) => Err(AppError::io(path, err)),
    }
}

/// Back up, then write atomically.
fn save(path: &Path, doc: &Map) -> Result<()> {
    if path.is_file() {
        let backup = path.with_file_name(format!(
            "{BACKUP_PREFIX}{}.bak",
            chrono::Utc::now().format("%Y%m%d-%H%M%S%3f")
        ));
        std::fs::copy(path, &backup).at(&backup)?;
        prune_backups(path.parent().expect("file has a parent"));
    }
    fsutil::write_atomic(path, &vdf::write(doc))
}

fn prune_backups(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut backups: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with(BACKUP_PREFIX))
        })
        .collect();
    backups.sort(); // the name ends in a sortable timestamp
    for old in backups.iter().rev().skip(BACKUPS_KEPT) {
        let _ = std::fs::remove_file(old);
    }
}

/// Renumber entries "0", "1", ... as Steam expects.
fn renumber(list: &mut Map) {
    for (index, (key, _)) in list.0.iter_mut().enumerate() {
        *key = index.to_string();
    }
}

/// Add or refresh `app`'s shortcut for each account. Steam must be closed.
pub fn add(steam_dir: &Path, app: &InstalledApp, account_ids: &[u32]) -> Result<()> {
    let exe = quoted(&app.dir.join(&app.exe));
    let start_dir = quoted(&app.dir);
    for &account in account_ids {
        let path = shortcuts_file(steam_dir, account);
        let mut doc = load(&path)?;
        let list = doc.map_mut("shortcuts").expect("load checks the list");
        let existing = list.0.iter_mut().find_map(|(_, v)| match v {
            Value::Map(m) if m.str("StartDir").is_some_and(|d| same_dir(d, &app.dir)) => Some(m),
            _ => None,
        });
        match existing {
            // Keep appid, launch options, tags and play time; Steam keys
            // artwork and stats on the appid.
            Some(entry) => {
                entry.set("AppName", Value::Str(app.name.clone()));
                entry.set("Exe", Value::Str(exe.clone()));
                entry.set("StartDir", Value::Str(start_dir.clone()));
                entry.set("icon", Value::Str(exe.clone()));
            }
            None => {
                let appid = shortcut_appid(&exe, &app.name);
                let mut entry = Map::default();
                entry.set("appid", Value::Int(appid as i32));
                entry.set("AppName", Value::Str(app.name.clone()));
                entry.set("Exe", Value::Str(exe.clone()));
                entry.set("StartDir", Value::Str(start_dir.clone()));
                entry.set("icon", Value::Str(exe.clone()));
                entry.set("ShortcutPath", Value::Str(String::new()));
                entry.set("LaunchOptions", Value::Str(String::new()));
                entry.set("IsHidden", Value::Int(0));
                entry.set("AllowDesktopConfig", Value::Int(1));
                entry.set("AllowOverlay", Value::Int(1));
                entry.set("OpenVR", Value::Int(0));
                entry.set("Devkit", Value::Int(0));
                entry.set("DevkitGameID", Value::Str(String::new()));
                entry.set("DevkitOverrideAppID", Value::Int(0));
                entry.set("LastPlayTime", Value::Int(0));
                entry.set("FlatpakAppID", Value::Str(String::new()));
                entry.set(
                    "tags",
                    Value::Map(Map(vec![("0".into(), Value::Str("RecompHub".into()))])),
                );
                list.0.push((String::new(), Value::Map(entry)));
            }
        }
        renumber(list);
        save(&path, &doc)?;
        tracing::info!(id = %app.id, account, "added to Steam");
    }
    Ok(())
}

/// Remove `app`'s shortcut from each account. Steam must be closed.
pub fn remove(steam_dir: &Path, app: &InstalledApp, account_ids: &[u32]) -> Result<()> {
    for &account in account_ids {
        let path = shortcuts_file(steam_dir, account);
        if !path.is_file() {
            continue;
        }
        let mut doc = load(&path)?;
        let list = doc.map_mut("shortcuts").expect("load checks the list");
        let before = list.0.len();
        list.0.retain(|(_, v)| match v {
            Value::Map(m) => !m.str("StartDir").is_some_and(|d| same_dir(d, &app.dir)),
            _ => true,
        });
        if list.0.len() != before {
            renumber(list);
            save(&path, &doc)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(dir: &Path, exe: &str) -> InstalledApp {
        InstalledApp {
            id: "zelda64recomp".into(),
            name: "Zelda 64: Recompiled".into(),
            version: "v1".into(),
            asset_name: "a.zip".into(),
            asset_id: 1,
            asset_sha256: None,
            dir: dir.to_path_buf(),
            exe: exe.into(),
            installed_at: chrono::Utc::now(),
            updated_at: None,
            files: Default::default(),
            previous: None,
            revision: "r".into(),
            shortcuts: Vec::new(),
            steam_users: Vec::new(),
        }
    }

    fn entries(steam: &Path, account: u32) -> Vec<Map> {
        let doc = vdf::parse(&std::fs::read(shortcuts_file(steam, account)).unwrap()).unwrap();
        doc.map("shortcuts")
            .unwrap()
            .0
            .iter()
            .map(|(_, v)| match v {
                Value::Map(m) => m.clone(),
                other => panic!("{other:?}"),
            })
            .collect()
    }

    #[test]
    fn crc32_matches_reference() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert!(shortcut_appid("\"a.exe\"", "A") & 0x8000_0000 != 0);
    }

    #[test]
    fn add_creates_file_and_never_duplicates() {
        let steam = tempfile::tempdir().unwrap();
        let game = steam.path().join("Games").join("zelda");
        let first = app(&game, "Zelda64Recompiled.exe");

        add(steam.path(), &first, &[42]).unwrap();
        let list = entries(steam.path(), 42);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].str("AppName"), Some("Zelda 64: Recompiled"));
        assert!(list[0]
            .str("Exe")
            .unwrap()
            .ends_with("Zelda64Recompiled.exe\""));
        let appid = list[0].get("appid").cloned();

        // Same game after an update with a renamed executable.
        add(steam.path(), &app(&game, "Zelda64.exe"), &[42]).unwrap();
        let list = entries(steam.path(), 42);
        assert_eq!(list.len(), 1);
        assert!(list[0].str("Exe").unwrap().ends_with("Zelda64.exe\""));
        assert_eq!(list[0].get("appid").cloned(), appid, "appid is kept");

        let backups = std::fs::read_dir(shortcuts_file(steam.path(), 42).parent().unwrap())
            .unwrap()
            .filter(|e| {
                e.as_ref()
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with(BACKUP_PREFIX)
            })
            .count();
        assert_eq!(backups, 1, "backup before the second write");
    }

    #[test]
    fn keeps_other_shortcuts_and_removes_only_ours() {
        let steam = tempfile::tempdir().unwrap();
        let path = shortcuts_file(steam.path(), 7);
        let mut other = Map::default();
        other.set("appid", Value::Int(123));
        other.set("AppName", Value::Str("Other game".into()));
        other.set("StartDir", Value::Str("\"D:\\Other\\\"".into()));
        other.set("LaunchOptions", Value::Str("-fullscreen".into()));
        let doc = Map(vec![(
            "shortcuts".into(),
            Value::Map(Map(vec![("0".into(), Value::Map(other.clone()))])),
        )]);
        fsutil::write_atomic(&path, &vdf::write(&doc)).unwrap();

        let ours = app(&steam.path().join("zelda"), "z.exe");
        add(steam.path(), &ours, &[7]).unwrap();
        let list = entries(steam.path(), 7);
        assert_eq!(list.len(), 2);
        assert_eq!(list[0], other, "foreign entry untouched");

        remove(steam.path(), &ours, &[7]).unwrap();
        assert_eq!(entries(steam.path(), 7), vec![other]);
    }

    #[test]
    fn refuses_files_it_cannot_reproduce() {
        let steam = tempfile::tempdir().unwrap();
        let path = shortcuts_file(steam.path(), 1);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"\x00shortcuts\x00\x05weird\x00x\x00\x08\x08").unwrap();
        let before = std::fs::read(&path).unwrap();
        assert!(add(steam.path(), &app(steam.path(), "z.exe"), &[1]).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }

    #[test]
    fn backups_are_pruned() {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..5 {
            std::fs::write(
                dir.path().join(format!("{BACKUP_PREFIX}2026010{i}.bak")),
                b"",
            )
            .unwrap();
        }
        prune_backups(dir.path());
        let mut left: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        left.sort();
        assert_eq!(left.len(), BACKUPS_KEPT);
        assert!(left[0].ends_with("20260102.bak"));
    }

    #[test]
    fn reads_login_users_most_recent_first() {
        let steam = tempfile::tempdir().unwrap();
        let config = steam.path().join("config");
        std::fs::create_dir_all(&config).unwrap();
        let a = STEAM_ID_BASE + 5;
        let b = STEAM_ID_BASE + 9;
        std::fs::write(
            config.join("loginusers.vdf"),
            format!(
                "\"users\"\n{{\n\"{a}\" {{ \"AccountName\" \"old\" \"PersonaName\" \"Old\" \"MostRecent\" \"0\" \"Timestamp\" \"200\" }}\n\"{b}\" {{ \"AccountName\" \"me\" \"PersonaName\" \"Me\" \"MostRecent\" \"1\" \"Timestamp\" \"100\" }}\n}}\n"
            ),
        )
        .unwrap();
        let users = users(steam.path()).unwrap();
        assert_eq!(users.len(), 2);
        assert_eq!(users[0].account_id, 9);
        assert!(users[0].most_recent);
        assert_eq!(users[1].persona_name, "Old");
        assert!(status(None).users.is_empty());
    }
}
