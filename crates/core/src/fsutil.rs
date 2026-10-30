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
