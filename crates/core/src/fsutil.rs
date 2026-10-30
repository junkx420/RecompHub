//! Small file system helpers.

use crate::error::{IoContext, Result};
use std::path::Path;

/// Write `data` to `path` atomically: write a sibling temp file, then rename.
/// A crash never leaves a half-written file behind.
pub fn write_atomic(path: &Path, data: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).at(parent)?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = std::path::PathBuf::from(tmp);
    {
        use std::io::Write;
        let mut file = std::fs::File::create(&tmp).at(&tmp)?;
        file.write_all(data).at(&tmp)?;
        file.sync_all().at(&tmp)?;
    }
    std::fs::rename(&tmp, path).at(path)
}

/// Rename `from` to `to`, creating `to`'s parent. A file another process
/// holds open (antivirus scan, a game that is just closing) is retried
/// 5 times with 100 ms to 1.6 s backoff.
pub fn rename_retry(from: &Path, to: &Path) -> Result<()> {
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent).at(parent)?;
    }
    let mut delay = std::time::Duration::from_millis(100);
    for attempt in 0.. {
        match std::fs::rename(from, to) {
            Ok(()) => return Ok(()),
            Err(err) if attempt < 5 && is_locked(&err) => {
                tracing::debug!(?from, %err, attempt, "rename blocked, retrying");
                std::thread::sleep(delay);
                delay *= 2;
            }
            Err(err) => return Err(crate::AppError::io(from, err)),
        }
    }
    unreachable!("loop returns")
}

/// Sharing violation, lock violation or access denied (Windows reports a
/// file in use by another process as one of these).
fn is_locked(err: &std::io::Error) -> bool {
    matches!(err.raw_os_error(), Some(5 | 32 | 33))
        || err.kind() == std::io::ErrorKind::PermissionDenied
}

/// Remove `dir` and every empty parent up to (not including) `stop`.
pub fn remove_empty_dirs(dir: &Path, stop: &Path) {
    let mut current = dir;
    while current != stop && current.starts_with(stop) {
        if std::fs::remove_dir(current).is_err() {
            break;
        }
        match current.parent() {
            Some(parent) => current = parent,
            None => break,
        }
    }
}

pub fn write_json<T: serde::Serialize>(path: &Path, value: &T) -> Result<()> {
    let data = serde_json::to_vec_pretty(value).map_err(|e| crate::AppError::InvalidInput {
        message: e.to_string(),
    })?;
    write_atomic(path, &data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_write_replaces_content_and_leaves_no_temp() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("sub").join("a.json");
        write_atomic(&file, b"one").unwrap();
        write_atomic(&file, b"two").unwrap();
        assert_eq!(std::fs::read(&file).unwrap(), b"two");
        let entries: Vec<_> = std::fs::read_dir(file.parent().unwrap()).unwrap().collect();
        assert_eq!(entries.len(), 1);
    }
}
