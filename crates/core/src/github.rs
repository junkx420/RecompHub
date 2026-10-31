//! GitHub releases API (subset).

use crate::catalog::{select_asset, CatalogEntry};
use crate::error::{AppError, Result};
use crate::http::Http;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Release {
    pub tag_name: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub prerelease: bool,
    #[serde(default)]
    pub draft: bool,
    #[serde(default)]
    pub published_at: Option<DateTime<Utc>>,
    pub html_url: String,
    pub assets: Vec<Asset>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Asset {
    pub id: u64,
    pub name: String,
    pub size: u64,
    pub browser_download_url: String,
    pub updated_at: DateTime<Utc>,
    /// `sha256:<hex>`, present on assets uploaded since mid-2025.
    #[serde(default)]
    pub digest: Option<String>,
}

impl Release {
    /// The single asset matching the entry's `asset_pattern`.
    pub fn asset_for(&self, entry: &CatalogEntry) -> Result<&Asset> {
        let names: Vec<&str> = self.assets.iter().map(|a| a.name.as_str()).collect();
        let name = select_asset(&entry.asset_pattern, &names)?;
        Ok(self
            .assets
            .iter()
            .find(|a| a.name == name)
            .expect("selected name comes from assets"))
    }
}

/// Fetch the release the entry points to: latest, its fixed tag, or with
/// `include_prereleases` the newest published release of any kind.
pub async fn fetch_release(http: &Http, entry: &CatalogEntry) -> Result<Release> {
    let url = entry.release_api_url();
    if entry.include_prereleases && entry.release_tag.is_none() {
        return newest_release(http, &url).await;
    }
    fetch_release_from(http, &url).await
}

async fn newest_release(http: &Http, url: &str) -> Result<Release> {
    let fetched = http.get_cached(url, true).await?;
    let releases: Vec<Release> =
        serde_json::from_slice(&fetched.body).map_err(|e| AppError::Network {
            message: format!("unexpected GitHub response from {url}: {e}"),
        })?;
    // GitHub lists newest first; drafts only appear for repo owners.
    releases
        .into_iter()
        .find(|r| !r.draft)
        .ok_or_else(|| AppError::NotFound {
            what: format!("{url}: no published release"),
        })
}

pub async fn fetch_release_from(http: &Http, url: &str) -> Result<Release> {
    let fetched = http.get_cached(url, true).await?;
    serde_json::from_slice(&fetched.body).map_err(|e| AppError::Network {
        message: format!("unexpected GitHub response from {url}: {e}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn release_json() -> serde_json::Value {
        serde_json::json!({
            "tag_name": "v1.0.0",
            "html_url": "https://github.com/o/r/releases/tag/v1.0.0",
            "assets": [
                {"id": 1, "name": "Game-v1.0.0-Windows.zip", "size": 10,
                 "browser_download_url": "https://example.com/w.zip",
                 "updated_at": "2026-01-01T00:00:00Z", "digest": "sha256:00"},
                {"id": 2, "name": "Game-v1.0.0-Linux.zip", "size": 10,
                 "browser_download_url": "https://example.com/l.zip",
                 "updated_at": "2026-01-01T00:00:00Z"}
            ]
        })
    }

    #[tokio::test]
    async fn fetches_release_and_selects_asset() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/o/r/releases/latest"))
            .respond_with(ResponseTemplate::new(200).set_body_json(release_json()))
            .mount(&server)
            .await;
        let dir = tempfile::tempdir().unwrap();
        let http = Http::new(dir.path()).unwrap();
        let release = fetch_release_from(
            &http,
            &format!("{}/repos/o/r/releases/latest", server.uri()),
        )
        .await
        .unwrap();
        assert_eq!(release.tag_name, "v1.0.0");

        let (entries, _) = crate::catalog::parse(
            crate::catalog::EMBEDDED_CATALOG.as_bytes(),
            Default::default(),
        )
        .unwrap();
        let mut entry = entries[0].clone();
        entry.asset_pattern = r"(?i)-Windows\.zip$".into();
        let asset = release.asset_for(&entry).unwrap();
        assert_eq!(asset.id, 1);
        assert_eq!(asset.digest.as_deref(), Some("sha256:00"));
    }
}
