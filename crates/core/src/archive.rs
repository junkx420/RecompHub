//! Unpacking release assets (zip, 7z, or a bare `.exe`).
//!
//! Every entry name is checked before anything is written: absolute paths,
//! drive letters and `..` are rejected ("zip slip"), so an archive can never
//! write outside its target folder. Symbolic links are skipped.
//!
//! Many releases wrap everything in one top-level folder; [`content_root`]
//! finds the folder that actually holds the files.

use crate::error::{AppError, IoContext, Result};
use crate::job::{CancelFlag, Progress, ProgressFn};
use std::fs::File;
use std::io::Read;
use std::path::{Component, Path, PathBuf};

const ZIP_MAGIC: &[u8] = b"PK\x03\x04";
const ZIP_EMPTY_MAGIC: &[u8] = b"PK\x05\x06";
const SEVEN_Z_MAGIC: &[u8] = b"7z\xBC\xAF\x27\x1C";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Zip,
    SevenZ,
    Executable,
}

fn detect(path: &Path, name: &str) -> Result<Kind> {
    let mut head = [0u8; 6];
    let mut file = File::open(path).at(path)?;
    let n = file.read(&mut head).at(path)?;
    let head = &head[..n];
    if head.starts_with(ZIP_MAGIC) || head.starts_with(ZIP_EMPTY_MAGIC) {
        Ok(Kind::Zip)
    } else if head.starts_with(SEVEN_Z_MAGIC) {
        Ok(Kind::SevenZ)
    } else if head.starts_with(b"MZ") && name.to_ascii_lowercase().ends_with(".exe") {
        Ok(Kind::Executable)
    } else {
        Err(AppError::UnsupportedArchive { name: name.into() })
    }
}

/// Join an archive entry name onto `dest`, refusing anything that could
/// leave `dest`. Returns `None` for names that are only `.`/separators.
pub fn safe_join(dest: &Path, entry: &str) -> Result<Option<PathBuf>> {
    let unsafe_entry = || AppError::UnsafeArchivePath {
        entry: entry.to_owned(),
    };
    let normalized = entry.replace('\\', "/");
    if normalized.starts_with('/') || normalized.contains(':') || normalized.contains('\0') {
        return Err(unsafe_entry());
    }
    let mut out = dest.to_path_buf();
    let mut any = false;
    for component in Path::new(&normalized).components() {
        match component {
            Component::Normal(part) => {
                out.push(part);
                any = true;
            }
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(unsafe_entry())
            }
        }
    }
    Ok(any.then_some(out))
}

/// Unpack `archive` (originally named `name`) into the empty folder `dest`.
/// Blocking; run it on a blocking thread.
pub fn extract(
    archive: &Path,
    name: &str,
    dest: &Path,
    cancel: &CancelFlag,
    progress: ProgressFn<'_>,
) -> Result<()> {
    std::fs::create_dir_all(dest).at(dest)?;
    match detect(archive, name)? {
        Kind::Zip => extract_zip(archive, dest, cancel, progress),
        Kind::SevenZ => extract_7z(archive, dest, cancel, progress),
        Kind::Executable => {
            let target = safe_join(dest, name)?.ok_or_else(|| AppError::UnsafeArchivePath {
                entry: name.to_owned(),
            })?;
            progress(Progress::Extracting { done: 0, total: 1 });
            std::fs::copy(archive, &target).at(&target)?;
            progress(Progress::Extracting { done: 1, total: 1 });
            Ok(())
        }
    }
}

fn archive_error(err: impl std::fmt::Display) -> AppError {
    AppError::ArchiveInvalid {
        message: err.to_string(),
    }
}

fn write_entry(target: &Path, reader: &mut dyn Read) -> Result<()> {
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).at(parent)?;
    }
    let mut out = File::create(target).at(target)?;
    std::io::copy(reader, &mut out).at(target)?;
    Ok(())
}

fn extract_zip(
    path: &Path,
    dest: &Path,
    cancel: &CancelFlag,
    progress: ProgressFn<'_>,
) -> Result<()> {
    let file = File::open(path).at(path)?;
    let mut zip = zip::ZipArchive::new(file).map_err(archive_error)?;
    let total = zip.len() as u64;
    // Validate every name first, so a bad archive writes nothing at all.
    for i in 0..zip.len() {
        let entry = zip.by_index_raw(i).map_err(archive_error)?;
        safe_join(dest, &entry.name().map_err(archive_error)?)?;
    }
    for i in 0..zip.len() {
        cancel.check()?;
        let mut entry = zip.by_index(i).map_err(archive_error)?;
        let name = entry.name().map_err(archive_error)?.into_owned();
        if entry.is_symlink() {
            tracing::warn!(%name, "skipping symbolic link in archive");
        } else if let Some(target) = safe_join(dest, &name)? {
            if entry.is_dir() {
                std::fs::create_dir_all(&target).at(&target)?;
            } else {
                write_entry(&target, &mut entry)?;
            }
        }
        progress(Progress::Extracting {
            done: i as u64 + 1,
            total,
        });
    }
    Ok(())
}

fn extract_7z(
    path: &Path,
    dest: &Path,
    cancel: &CancelFlag,
    progress: ProgressFn<'_>,
) -> Result<()> {
    let file = File::open(path).at(path)?;
    let mut reader = sevenz_rust2::ArchiveReader::new(file, sevenz_rust2::Password::empty())
        .map_err(archive_error)?;
    let total = reader.archive().files.len() as u64;
    for entry in &reader.archive().files {
        safe_join(dest, entry.name())?;
    }

    let mut done = 0u64;
    let mut failure: Option<AppError> = None;
    reader
        .for_each_entries(|entry, data| {
            if cancel.is_cancelled() {
                failure = Some(AppError::Cancelled);
                return Ok(false);
            }
            let result = (|| -> Result<()> {
                let Some(target) = safe_join(dest, entry.name())? else {
                    return Ok(());
                };
                if entry.is_directory() {
                    std::fs::create_dir_all(&target).at(&target)
                } else {
                    write_entry(&target, data)
                }
            })();
            if let Err(err) = result {
                failure = Some(err);
                return Ok(false);
            }
            done += 1;
            progress(Progress::Extracting { done, total });
            Ok(true)
        })
        .map_err(archive_error)?;
    match failure {
        Some(err) => Err(err),
        None => Ok(()),
    }
}

/// The folder holding the actual files: `dir` itself, or its only child if
/// that child is a folder (repeated, for nested single folders).
pub fn content_root(dir: &Path) -> Result<PathBuf> {
    let mut current = dir.to_path_buf();
    loop {
        let mut entries = std::fs::read_dir(&current).at(&current)?;
        let first = match entries.next() {
            Some(entry) => entry.at(&current)?,
            None => return Ok(current),
        };
        if entries.next().is_some() || !first.file_type().at(first.path())?.is_dir() {
            return Ok(current);
        }
        current = first.path();
    }
}

/// All regular files below `dir` as `/`-separated relative paths, sorted.
pub fn list_files(dir: &Path) -> Result<Vec<String>> {
    fn walk(base: &Path, dir: &Path, out: &mut Vec<String>) -> Result<()> {
        for entry in std::fs::read_dir(dir).at(dir)? {
            let entry = entry.at(dir)?;
            let path = entry.path();
            let kind = entry.file_type().at(&path)?;
            if kind.is_dir() {
                walk(base, &path, out)?;
            } else if kind.is_file() {
                let rel = path.strip_prefix(base).expect("walk stays below base");
                let parts: Vec<_> = rel.iter().map(|p| p.to_string_lossy()).collect();
                out.push(parts.join("/"));
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    walk(dir, dir, &mut out)?;
    out.sort();
    Ok(out)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::io::Write;

    fn no_progress(_: Progress) {}

    /// Build a zip in memory from `(name, contents)`; `None` marks a folder.
    pub(crate) fn make_zip(entries: &[(&str, Option<&[u8]>)]) -> Vec<u8> {
        let mut buf = std::io::Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut buf);
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated);
            for (name, data) in entries {
                match data {
                    Some(data) => {
                        zip.start_file(*name, options).unwrap();
                        zip.write_all(data).unwrap();
                    }
                    None => zip.add_directory(*name, options).unwrap(),
                }
            }
            zip.finish().unwrap();
        }
        buf.into_inner()
    }

    #[test]
    fn safe_join_rejects_escapes() {
        let dest = Path::new("dest");
        assert_eq!(
            safe_join(dest, "a/b.txt").unwrap(),
            Some(dest.join("a").join("b.txt"))
        );
        assert_eq!(
            safe_join(dest, "./a\\b").unwrap(),
            Some(dest.join("a").join("b"))
        );
        assert_eq!(safe_join(dest, "./").unwrap(), None);
        for bad in [
            "../x",
            "a/../../x",
            "/etc/passwd",
            "\\x",
            "C:\\x",
            "C:x",
            "a\\..\\..\\x",
        ] {
            assert!(
                matches!(
                    safe_join(dest, bad),
                    Err(AppError::UnsafeArchivePath { .. })
                ),
                "{bad} should be rejected"
            );
        }
    }

    #[test]
    fn extracts_zip_and_finds_single_root() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("a.zip");
        std::fs::write(
            &archive,
            make_zip(&[
                ("Game/", None),
                ("Game/game.exe", Some(b"MZexe")),
                ("Game/assets/data.bin", Some(b"data")),
            ]),
        )
        .unwrap();
        let out = dir.path().join("out");
        extract(&archive, "a.zip", &out, &CancelFlag::new(), &no_progress).unwrap();

        let root = content_root(&out).unwrap();
        assert_eq!(root, out.join("Game"));
        assert_eq!(
            list_files(&root).unwrap(),
            vec!["assets/data.bin".to_string(), "game.exe".to_string()]
        );
    }

    #[test]
    fn zip_slip_writes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let archive = dir.path().join("evil.zip");
        std::fs::write(
            &archive,
            make_zip(&[("ok.txt", Some(b"ok")), ("../evil.txt", Some(b"x"))]),
        )
        .unwrap();
        let out = dir.path().join("out");
        let err =
            extract(&archive, "evil.zip", &out, &CancelFlag::new(), &no_progress).unwrap_err();
        assert!(matches!(err, AppError::UnsafeArchivePath { .. }));
        assert!(!dir.path().join("evil.txt").exists());
        assert!(!out.join("ok.txt").exists());
    }

    #[test]
    fn extracts_7z() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir_all(src.join("sub")).unwrap();
        std::fs::write(src.join("game.exe"), b"MZexe").unwrap();
        std::fs::write(src.join("sub").join("x.txt"), b"x").unwrap();
        let archive = dir.path().join("a.7z");
        sevenz_rust2::compress_to_path(&src, &archive).unwrap();

        let out = dir.path().join("out");
        extract(&archive, "a.7z", &out, &CancelFlag::new(), &no_progress).unwrap();
        let files = list_files(&content_root(&out).unwrap()).unwrap();
        assert!(files.contains(&"game.exe".to_string()), "{files:?}");
        assert!(files.contains(&"sub/x.txt".to_string()), "{files:?}");
    }

    #[test]
    fn bare_exe_is_copied() {
        let dir = tempfile::tempdir().unwrap();
        let asset = dir.path().join("123-Tool.exe");
        std::fs::write(&asset, b"MZ\x90\x00rest").unwrap();
        let out = dir.path().join("out");
        extract(&asset, "Tool.exe", &out, &CancelFlag::new(), &no_progress).unwrap();
        assert_eq!(list_files(&out).unwrap(), vec!["Tool.exe".to_string()]);
    }

    #[test]
    fn unknown_format_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let asset = dir.path().join("x.tar.gz");
        std::fs::write(&asset, b"\x1f\x8bgzip").unwrap();
        let err = extract(
            &asset,
            "x.tar.gz",
            &dir.path().join("out"),
            &CancelFlag::new(),
            &no_progress,
        )
        .unwrap_err();
        assert!(matches!(err, AppError::UnsupportedArchive { .. }));
    }
}
