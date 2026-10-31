//! Resumable asset downloads with SHA-256 verification.
//!
//! A download writes `<name>.part` next to `<name>.part.json`, which records
//! what the partial file belongs to. A later attempt for the same asset
//! (same URL, size and key) resumes with an HTTP `Range` request; anything
//! else starts over. The finished file is checked against the expected size
//! and, when GitHub provides one, the asset's `sha256:` digest, and only then
//! renamed to its final name.

use crate::error::{AppError, IoContext, Result};
use crate::http::{check_status, Http};
use crate::job::{CancelFlag, Progress, ProgressFn};
use crate::{fsutil, github::Asset};
use reqwest::header;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;

/// Report progress at most every this many bytes.
const PROGRESS_STEP: u64 = 256 * 1024;

#[derive(Debug, Clone)]
pub struct DownloadSpec {
    pub url: String,
    pub file_name: String,
    pub size: u64,
    /// Hex SHA-256 the file must have, if known.
    pub sha256: Option<String>,
    /// Identifies the remote file version; a changed key discards partial data.
    pub key: String,
}

impl DownloadSpec {
    pub fn for_asset(asset: &Asset) -> Self {
        Self {
            url: asset.browser_download_url.clone(),
            file_name: format!("{}-{}", asset.id, sanitize_file_name(&asset.name)),
            size: asset.size,
            sha256: asset.digest.as_deref().and_then(parse_sha256_digest),
            key: format!("{}@{}", asset.id, asset.updated_at.to_rfc3339()),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
struct PartMeta {
    url: String,
    size: u64,
    key: String,
}

/// `sha256:<64 hex>` to lowercase hex; other algorithms are not checked.
pub fn parse_sha256_digest(digest: &str) -> Option<String> {
    let hex = digest.strip_prefix("sha256:")?;
    (hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_hexdigit()))
        .then(|| hex.to_ascii_lowercase())
}

/// Keep asset names usable as Windows file names.
pub(crate) fn sanitize_file_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_control() || r#"<>:"/\|?*"#.contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect();
    let trimmed = cleaned.trim_end_matches(['.', ' ']);
    if trimmed.is_empty() {
        "download".into()
    } else {
        trimmed.into()
    }
}

/// Download `spec` into `dir` and return the path of the verified file.
pub async fn download(
    http: &Http,
    spec: &DownloadSpec,
    dir: &Path,
    cancel: &CancelFlag,
    progress: ProgressFn<'_>,
) -> Result<PathBuf> {
    tokio::fs::create_dir_all(dir).await.at(dir)?;
    let final_path = dir.join(&spec.file_name);
    let part_path = dir.join(format!("{}.part", spec.file_name));
    let meta_path = dir.join(format!("{}.part.json", spec.file_name));
    let meta = PartMeta {
        url: spec.url.clone(),
        size: spec.size,
        key: spec.key.clone(),
    };

    let mut offset = resumable_offset(&part_path, &meta_path, &meta).await;
    if offset == 0 {
        let _ = tokio::fs::remove_file(&part_path).await;
        fsutil::write_json(&meta_path, &meta)?;
    }

    // A complete part file from an earlier run only needs verifying.
    if offset < spec.size || spec.size == 0 {
        offset = fetch_into(http, spec, &part_path, offset, cancel, progress).await?;
    }
    progress(Progress::Downloading {
        done: offset,
        total: spec.size,
    });
    cancel.check()?;

    progress(Progress::Verifying);
    if let Err(err) = verify(&part_path, spec).await {
        // Corrupt data must not be resumed next time.
        let _ = tokio::fs::remove_file(&part_path).await;
        let _ = tokio::fs::remove_file(&meta_path).await;
        return Err(err);
    }
    tokio::fs::rename(&part_path, &final_path)
        .await
        .at(&final_path)?;
    let _ = tokio::fs::remove_file(&meta_path).await;
    Ok(final_path)
}

/// Bytes already on disk for the same remote file, or 0.
async fn resumable_offset(part_path: &Path, meta_path: &Path, meta: &PartMeta) -> u64 {
    let Ok(bytes) = tokio::fs::read(meta_path).await else {
        return 0;
    };
    let Ok(stored) = serde_json::from_slice::<PartMeta>(&bytes) else {
        return 0;
    };
    if &stored != meta {
        return 0;
    }
    match tokio::fs::metadata(part_path).await {
        Ok(m) if m.len() <= meta.size => m.len(),
        _ => 0,
    }
}

async fn fetch_into(
    http: &Http,
    spec: &DownloadSpec,
    part_path: &Path,
    mut offset: u64,
    cancel: &CancelFlag,
    progress: ProgressFn<'_>,
) -> Result<u64> {
    let mut request = http.download_client().get(&spec.url);
    if offset > 0 {
        request = request.header(header::RANGE, format!("bytes={offset}-"));
    }
    let mut response = request.send().await.map_err(AppError::network)?;
    if response.status() == StatusCode::RANGE_NOT_SATISFIABLE {
        // Our part file is already the whole thing; let verification decide.
        return Ok(offset);
    }
    check_status(&response)?;

    let resumed = offset > 0 && response.status() == StatusCode::PARTIAL_CONTENT;
    let mut file = if resumed {
        tracing::info!(url = %spec.url, offset, "resuming download");
        tokio::fs::OpenOptions::new()
            .append(true)
            .open(part_path)
            .await
            .at(part_path)?
    } else {
        // Server ignored the range (or this is a fresh start): begin anew.
        offset = 0;
        tokio::fs::File::create(part_path).await.at(part_path)?
    };

    let mut reported = offset;
    progress(Progress::Downloading {
        done: offset,
        total: spec.size,
    });
    loop {
        if cancel.is_cancelled() {
            file.flush().await.at(part_path)?;
            return Err(AppError::Cancelled);
        }
        let Some(chunk) = response.chunk().await.map_err(AppError::network)? else {
            break;
        };
        offset += chunk.len() as u64;
        if offset > spec.size {
            return Err(AppError::SizeMismatch {
                expected: spec.size,
                actual: offset,
            });
        }
        file.write_all(&chunk).await.at(part_path)?;
        if offset - reported >= PROGRESS_STEP {
            reported = offset;
            progress(Progress::Downloading {
                done: offset,
                total: spec.size,
            });
        }
    }
    file.flush().await.at(part_path)?;
    file.sync_all().await.at(part_path)?;
    Ok(offset)
}

async fn verify(path: &Path, spec: &DownloadSpec) -> Result<()> {
    let actual_size = tokio::fs::metadata(path).await.at(path)?.len();
    if actual_size != spec.size {
        return Err(AppError::SizeMismatch {
            expected: spec.size,
            actual: actual_size,
        });
    }
    if let Some(expected) = &spec.sha256 {
        let actual = sha256_file(path.to_path_buf()).await?;
        if &actual != expected {
            return Err(AppError::ChecksumMismatch {
                expected: expected.clone(),
                actual,
            });
        }
    } else {
        tracing::warn!(url = %spec.url, "asset has no sha256 digest; only the size was checked");
    }
    Ok(())
}

/// Hex SHA-256 of a file, computed off the async runtime.
pub async fn sha256_file(path: PathBuf) -> Result<String> {
    tokio::task::spawn_blocking(move || sha256_file_blocking(&path))
        .await
        .map_err(|e| AppError::InvalidInput {
            message: format!("hash task failed: {e}"),
        })?
}

pub fn sha256_file_blocking(path: &Path) -> Result<String> {
    let mut file = std::fs::File::open(path).at(path)?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher).at(path)?;
    Ok(hex::encode(hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use wiremock::matchers::{header as header_is, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn body() -> Vec<u8> {
        (0..300_000u32).map(|i| (i % 251) as u8).collect()
    }

    fn spec(server: &MockServer, data: &[u8]) -> DownloadSpec {
        DownloadSpec {
            url: format!("{}/asset.zip", server.uri()),
            file_name: "1-asset.zip".into(),
            size: data.len() as u64,
            sha256: Some(hex::encode(Sha256::digest(data))),
            key: "1@t".into(),
        }
    }

    fn no_progress(_: Progress) {}

    #[test]
    fn parses_only_sha256_digests() {
        let hex = "AB".repeat(32);
        assert_eq!(
            parse_sha256_digest(&format!("sha256:{hex}")),
            Some("ab".repeat(32))
        );
        assert_eq!(parse_sha256_digest("sha256:00"), None);
        assert_eq!(parse_sha256_digest(&format!("sha512:{hex}")), None);
    }

    #[test]
    fn sanitizes_windows_file_names() {
        assert_eq!(sanitize_file_name("a:b/c?.zip"), "a_b_c_.zip");
        assert_eq!(sanitize_file_name("name. "), "name");
        assert_eq!(sanitize_file_name(".."), "download");
    }

    #[tokio::test]
    async fn downloads_and_verifies() {
        let server = MockServer::start().await;
        let data = body();
        Mock::given(method("GET"))
            .and(path("/asset.zip"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(data.clone()))
            .mount(&server)
            .await;
        let dir = tempfile::tempdir().unwrap();
        let http = Http::new(dir.path().join("cache")).unwrap();
        let seen = Mutex::new(Vec::new());
        let report = |p: Progress| seen.lock().unwrap().push(p);

        let file = download(
            &http,
            &spec(&server, &data),
            dir.path(),
            &CancelFlag::new(),
            &report,
        )
        .await
        .unwrap();
        assert_eq!(std::fs::read(&file).unwrap(), data);
        assert!(!dir.path().join("1-asset.zip.part").exists());
        assert!(!dir.path().join("1-asset.zip.part.json").exists());
        let seen = seen.into_inner().unwrap();
        assert!(seen.contains(&Progress::Verifying));
        assert!(seen.contains(&Progress::Downloading {
            done: data.len() as u64,
            total: data.len() as u64
        }));
    }

    #[tokio::test]
    async fn resumes_with_range_request() {
        let server = MockServer::start().await;
        let data = body();
        let half = 100_000usize;
        Mock::given(method("GET"))
            .and(path("/asset.zip"))
            .and(header_is("range", format!("bytes={half}-").as_str()))
            .respond_with(ResponseTemplate::new(206).set_body_bytes(data[half..].to_vec()))
            .expect(1)
            .mount(&server)
            .await;
        let dir = tempfile::tempdir().unwrap();
        let http = Http::new(dir.path().join("cache")).unwrap();
        let spec = spec(&server, &data);
        std::fs::write(dir.path().join("1-asset.zip.part"), &data[..half]).unwrap();
        fsutil::write_json(
            &dir.path().join("1-asset.zip.part.json"),
            &PartMeta {
                url: spec.url.clone(),
                size: spec.size,
                key: spec.key.clone(),
            },
        )
        .unwrap();

        let file = download(&http, &spec, dir.path(), &CancelFlag::new(), &no_progress)
            .await
            .unwrap();
        assert_eq!(std::fs::read(file).unwrap(), data);
    }

    #[tokio::test]
    async fn restarts_when_partial_belongs_to_other_version() {
        let server = MockServer::start().await;
        let data = body();
        Mock::given(method("GET"))
            .and(path("/asset.zip"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(data.clone()))
            .mount(&server)
            .await;
        let dir = tempfile::tempdir().unwrap();
        let http = Http::new(dir.path().join("cache")).unwrap();
        let spec = spec(&server, &data);
        std::fs::write(dir.path().join("1-asset.zip.part"), b"stale bytes").unwrap();
        fsutil::write_json(
            &dir.path().join("1-asset.zip.part.json"),
            &PartMeta {
                url: spec.url.clone(),
                size: spec.size,
                key: "1@older".into(),
            },
        )
        .unwrap();

        let file = download(&http, &spec, dir.path(), &CancelFlag::new(), &no_progress)
            .await
            .unwrap();
        assert_eq!(std::fs::read(file).unwrap(), data);
        let requests = server.received_requests().await.unwrap();
        assert!(requests.iter().all(|r| !r.headers.contains_key("range")));
    }

    #[tokio::test]
    async fn checksum_mismatch_discards_partial() {
        let server = MockServer::start().await;
        let data = body();
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(data.clone()))
            .mount(&server)
            .await;
        let dir = tempfile::tempdir().unwrap();
        let http = Http::new(dir.path().join("cache")).unwrap();
        let mut spec = spec(&server, &data);
        spec.sha256 = Some("0".repeat(64));

        let err = download(&http, &spec, dir.path(), &CancelFlag::new(), &no_progress)
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::ChecksumMismatch { .. }));
        assert!(!dir.path().join("1-asset.zip.part").exists());
        assert!(!dir.path().join("1-asset.zip").exists());
    }

    #[tokio::test]
    async fn cancel_keeps_partial_for_resume() {
        let server = MockServer::start().await;
        let data = body();
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(data.clone()))
            .mount(&server)
            .await;
        let dir = tempfile::tempdir().unwrap();
        let http = Http::new(dir.path().join("cache")).unwrap();
        let cancel = CancelFlag::new();
        cancel.cancel();

        let err = download(
            &http,
            &spec(&server, &data),
            dir.path(),
            &cancel,
            &no_progress,
        )
        .await
        .unwrap_err();
        assert!(matches!(err, AppError::Cancelled));
        assert!(dir.path().join("1-asset.zip.part.json").exists());
        assert!(!dir.path().join("1-asset.zip").exists());
    }
}
