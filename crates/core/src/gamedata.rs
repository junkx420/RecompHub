//! The user's own game data: checking and importing ROM dumps.
//!
//! For `copy_file` entries the user picks a file; RecompHub converts it to
//! big-endian `.z64` byte order if needed (from `.v64` or `.n64`), compares
//! SHA-1/MD5 with the catalog's known dumps, and only then copies it into
//! the game folder. Nothing is ever downloaded: the data comes from the user.

use crate::catalog::{CatalogEntry, GameData, KnownDump, LocalizedText};
use crate::error::{AppError, IoContext, Result};
use crate::fsutil;
use crate::installed::InstalledApp;
use md5::Md5;
use serde::Serialize;
use sha1::{Digest, Sha1};
use std::path::{Path, PathBuf};

/// Largest N64 cartridge is 64 MiB; anything far beyond is not a ROM.
const MAX_ROM_SIZE: u64 = 128 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum GameDataStatus {
    /// The port needs no game data.
    NotNeeded,
    /// The port asks for the data itself on first start.
    HandledByPort { hint: LocalizedText },
    /// RecompHub must copy a verified dump to `target`; none is there yet.
    Missing { target: PathBuf },
    /// A dump is in place; `dump` is set when it matches an accepted hash.
    Present {
        target: PathBuf,
        dump: Option<KnownDump>,
    },
}

/// Byte order of an N64 ROM, detected from its first four bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RomOrder {
    /// `.z64`, big-endian (the native order hashes refer to).
    Big,
    /// `.v64`, 16-bit words byte-swapped.
    ByteSwapped,
    /// `.n64`, 32-bit words little-endian.
    Little,
}

fn rom_order(data: &[u8]) -> Option<RomOrder> {
    match data.get(..4)? {
        [0x80, 0x37, 0x12, 0x40] => Some(RomOrder::Big),
        [0x37, 0x80, 0x40, 0x12] => Some(RomOrder::ByteSwapped),
        [0x40, 0x12, 0x37, 0x80] => Some(RomOrder::Little),
        _ => None,
    }
}

/// Convert ROM bytes to `.z64` order in place.
fn to_big_endian(data: &mut [u8], order: RomOrder) {
    match order {
        RomOrder::Big => {}
        RomOrder::ByteSwapped => data
            .as_chunks_mut::<2>()
            .0
            .iter_mut()
            .for_each(|c| c.swap(0, 1)),
        RomOrder::Little => data
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .for_each(|c| c.reverse()),
    }
}

struct Hashes {
    sha1: String,
    md5: String,
}

fn hashes(data: &[u8]) -> Hashes {
    Hashes {
        sha1: hex::encode(Sha1::digest(data)),
        md5: hex::encode(Md5::digest(data)),
    }
}

impl KnownDump {
    fn matches(&self, h: &Hashes) -> bool {
        let sha1 = self
            .sha1
            .as_deref()
            .map(|s| s.eq_ignore_ascii_case(&h.sha1));
        let md5 = self.md5.as_deref().map(|s| s.eq_ignore_ascii_case(&h.md5));
        // Every hash the catalog gives must match, and at least one is given.
        match (sha1, md5) {
            (None, None) => false,
            (a, b) => a.unwrap_or(true) && b.unwrap_or(true),
        }
    }
}

/// Read a ROM, convert it to `.z64` order and identify it. Blocking.
fn read_rom(path: &Path) -> Result<(Vec<u8>, Hashes)> {
    let size = std::fs::metadata(path).at(path)?.len();
    if !(0x1000..=MAX_ROM_SIZE).contains(&size) {
        return Err(AppError::NotARom);
    }
    let mut data = std::fs::read(path).at(path)?;
    let order = rom_order(&data).ok_or(AppError::NotARom)?;
    to_big_endian(&mut data, order);
    let h = hashes(&data);
    Ok((data, h))
}

/// Identify a dump against the entry's accepted and known hashes.
fn identify<'a>(
    accepted: &'a [KnownDump],
    others: &[KnownDump],
    h: &Hashes,
) -> Result<&'a KnownDump> {
    if let Some(dump) = accepted.iter().find(|d| d.matches(h)) {
        return Ok(dump);
    }
    if let Some(dump) = others.iter().find(|d| d.matches(h)) {
        return Err(AppError::WrongDump {
            region: dump.region.clone(),
            version: dump.version.clone(),
        });
    }
    Err(AppError::UnknownDump {
        sha1: h.sha1.clone(),
        md5: h.md5.clone(),
    })
}

/// What the entry needs and whether it is in place. Blocking: hashes the
/// target file if there is one.
pub fn status(app: &InstalledApp, entry: &CatalogEntry) -> Result<GameDataStatus> {
    Ok(match &entry.game_data {
        GameData::None => GameDataStatus::NotNeeded,
        GameData::PortHandlesIt { hint } => GameDataStatus::HandledByPort { hint: hint.clone() },
        GameData::CopyFile {
            target,
            accepted_hashes,
            ..
        } => {
            let path = app.dir.join(target);
            if !path.is_file() {
                return Ok(GameDataStatus::Missing {
                    target: crate::paths::normalize(&path),
                });
            }
            let data = std::fs::read(&path).at(&path)?;
            let h = hashes(&data);
            GameDataStatus::Present {
                target: crate::paths::normalize(&path),
                dump: accepted_hashes.iter().find(|d| d.matches(&h)).cloned(),
            }
        }
    })
}

/// Check the user's file and copy it into the game folder. Blocking.
pub fn import(app: &InstalledApp, entry: &CatalogEntry, source: &Path) -> Result<KnownDump> {
    let GameData::CopyFile {
        target,
        accepted_hashes,
        other_known_hashes,
        ..
    } = &entry.game_data
    else {
        return Err(AppError::InvalidInput {
            message: format!("{} does not take game data from RecompHub", entry.id),
        });
    };
    let (data, h) = read_rom(source)?;
    let dump = identify(accepted_hashes, other_known_hashes, &h)?.clone();
    let path = app.dir.join(target);
    fsutil::write_atomic(&path, &data)?;
    tracing::info!(id = %app.id, region = %dump.region, version = %dump.version, "game data imported");
    Ok(dump)
}

/// Can the game start? `copy_file` entries need a dump in place.
pub fn ready_to_launch(app: &InstalledApp, entry: &CatalogEntry) -> Result<()> {
    if let GameData::CopyFile { target, .. } = &entry.game_data {
        if !app.dir.join(target).is_file() {
            return Err(AppError::GameDataMissing { id: app.id.clone() });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    /// A fake ROM in `.z64` order: valid header, deterministic body.
    fn z64() -> Vec<u8> {
        let mut data: Vec<u8> = (0..0x2000u32).map(|i| (i * 7 % 256) as u8).collect();
        data[..4].copy_from_slice(&[0x80, 0x37, 0x12, 0x40]);
        data
    }

    fn dump(data: &[u8], version: &str, md5_only: bool) -> KnownDump {
        let h = hashes(data);
        KnownDump {
            sha1: (!md5_only).then_some(h.sha1),
            md5: Some(h.md5),
            region: "US".into(),
            version: version.into(),
        }
    }

    fn setup(game_data: GameData) -> (tempfile::TempDir, InstalledApp, CatalogEntry) {
        let dir = tempfile::tempdir().unwrap();
        let (entries, _) = crate::catalog::parse(
            crate::catalog::EMBEDDED_CATALOG.as_bytes(),
            Default::default(),
        )
        .unwrap();
        let mut entry = entries
            .into_iter()
            .find(|e| e.id == "perfect-dark")
            .unwrap();
        entry.game_data = game_data;
        let app = InstalledApp {
            id: entry.id.clone(),
            name: entry.name.clone(),
            version: "v1".into(),
            asset_name: "a.zip".into(),
            asset_id: 1,
            asset_sha256: None,
            dir: dir.path().join("app"),
            exe: "pd.exe".into(),
            installed_at: chrono::Utc::now(),
            updated_at: None,
            files: BTreeMap::new(),
            previous: None,
            revision: "r".into(),
            shortcuts: Vec::new(),
            steam_users: Vec::new(),
        };
        std::fs::create_dir_all(&app.dir).unwrap();
        (dir, app, entry)
    }

    fn copy_file(accepted: Vec<KnownDump>, others: Vec<KnownDump>) -> GameData {
        GameData::CopyFile {
            target: "data/rom.z64".into(),
            accepted_hashes: accepted,
            other_known_hashes: others,
            hint: None,
        }
    }

    #[test]
    fn converts_v64_and_n64_to_z64() {
        let big = z64();
        let mut v64 = big.clone();
        v64.as_chunks_mut::<2>()
            .0
            .iter_mut()
            .for_each(|c| c.swap(0, 1));
        let mut n64 = big.clone();
        n64.as_chunks_mut::<4>()
            .0
            .iter_mut()
            .for_each(|c| c.reverse());
        for (mut data, order) in [
            (big.clone(), RomOrder::Big),
            (v64, RomOrder::ByteSwapped),
            (n64, RomOrder::Little),
        ] {
            assert_eq!(rom_order(&data), Some(order));
            to_big_endian(&mut data, order);
            assert_eq!(data, big);
        }
        assert_eq!(rom_order(b"PK\x03\x04"), None);
    }

    #[test]
    fn import_accepts_byte_swapped_dump_and_writes_z64() {
        let rom = z64();
        let (dir, app, entry) = setup(copy_file(vec![dump(&rom, "1.1", true)], vec![]));
        let mut v64 = rom.clone();
        v64.as_chunks_mut::<2>()
            .0
            .iter_mut()
            .for_each(|c| c.swap(0, 1));
        let source = dir.path().join("game.v64");
        std::fs::write(&source, &v64).unwrap();

        assert!(matches!(
            status(&app, &entry).unwrap(),
            GameDataStatus::Missing { .. }
        ));
        assert!(matches!(
            ready_to_launch(&app, &entry),
            Err(AppError::GameDataMissing { .. })
        ));
        let found = import(&app, &entry, &source).unwrap();
        assert_eq!(found.version, "1.1");
        assert_eq!(std::fs::read(app.dir.join("data/rom.z64")).unwrap(), rom);
        match status(&app, &entry).unwrap() {
            GameDataStatus::Present { dump: Some(d), .. } => assert_eq!(d.version, "1.1"),
            other => panic!("unexpected {other:?}"),
        }
        ready_to_launch(&app, &entry).unwrap();
    }

    #[test]
    fn known_wrong_dump_is_named() {
        let rom = z64();
        let mut other = rom.clone();
        other[100] ^= 0xff;
        let (dir, app, entry) = setup(copy_file(
            vec![dump(&rom, "1.1", false)],
            vec![dump(&other, "1.0", true)],
        ));
        let source = dir.path().join("old.z64");
        std::fs::write(&source, &other).unwrap();
        match import(&app, &entry, &source) {
            Err(AppError::WrongDump { version, .. }) => assert_eq!(version, "1.0"),
            other => panic!("unexpected {other:?}"),
        }
        assert!(!app.dir.join("data/rom.z64").exists());
    }

    #[test]
    fn unknown_dump_and_non_rom_are_rejected() {
        let rom = z64();
        let (dir, app, entry) = setup(copy_file(vec![dump(&rom, "1.1", false)], vec![]));
        let mut changed = rom.clone();
        changed[200] ^= 1;
        let source = dir.path().join("hack.z64");
        std::fs::write(&source, &changed).unwrap();
        assert!(matches!(
            import(&app, &entry, &source),
            Err(AppError::UnknownDump { .. })
        ));
        let zip = dir.path().join("rom.zip");
        std::fs::write(&zip, [b"PK\x03\x04".as_slice(), &[0; 0x2000]].concat()).unwrap();
        assert!(matches!(import(&app, &entry, &zip), Err(AppError::NotARom)));
    }

    #[test]
    fn other_modes_report_without_files() {
        let (_dir, app, entry) = setup(GameData::None);
        assert_eq!(status(&app, &entry).unwrap(), GameDataStatus::NotNeeded);
        ready_to_launch(&app, &entry).unwrap();
        assert!(import(&app, &entry, Path::new("x")).is_err());
    }
}
