//! HTTP client with an on-disk ETag cache.
//!
//! Each cached URL has two files in the cache directory, keyed by the SHA-256
//! of the URL: `<key>.meta.json` (ETag, Last-Modified, time) and `<key>.body`.
//! Requests send `If-None-Match`/`If-Modified-Since`; a `304` returns the
//! cached body. Conditional GitHub requests answered with `304` do not count
//! against the rate limit.

use crate::error::{AppError, IoContext, Result};
use crate::fsutil;
use chrono::{DateTime, Utc};
use reqwest::header::{self, HeaderMap, HeaderValue};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::sync::RwLock;
use std::time::Duration;

const GITHUB_API_HOST: &str = "api.github.com";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FetchSource {
    /// Fresh `200` from the server.
    Network,
    /// Server answered `304`, cached body is current.
    NotModified,
    /// Network failed, cached body may be outdated.
    StaleCache,
}

#[derive(Debug, Clone)]
pub struct Fetched {
    pub body: Vec<u8>,
    pub source: FetchSource,
    pub fetched_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
struct CacheMeta {
    url: String,
    etag: Option<String>,
    last_modified: Option<String>,
    fetched_at: DateTime<Utc>,
}

pub struct Http {
    client: reqwest::Client,
    download_client: reqwest::Client,
    cache_dir: PathBuf,
    github_token: RwLock<Option<String>>,
}

const USER_AGENT: &str = concat!("RecompHub/", env!("CARGO_PKG_VERSION"));

impl Http {
    pub fn new(cache_dir: impl Into<PathBuf>) -> Result<Self> {
        let client = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .connect_timeout(Duration::from_secs(15))
            .timeout(Duration::from_secs(60))
            .build()
            .map_err(AppError::network)?;
        // Large assets: no total timeout, only a stall timeout per read.
        let download_client = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .connect_timeout(Duration::from_secs(15))
            .read_timeout(Duration::from_secs(30))
            .build()
            .map_err(AppError::network)?;
        Ok(Self {
            client,
            download_client,
            cache_dir: cache_dir.into(),
            github_token: RwLock::new(None),
        })
    }

    pub fn client(&self) -> &reqwest::Client {
        &self.client
    }

    /// Client for release asset downloads. Never sends the GitHub token:
    /// asset URLs redirect to a CDN host.
    pub fn download_client(&self) -> &reqwest::Client {
        &self.download_client
    }

    /// Set or clear the GitHub token. It is only ever sent to api.github.com.
    pub fn set_github_token(&self, token: Option<String>) {
        *self.github_token.write().expect("token lock poisoned") =
            token.filter(|t| !t.trim().is_empty());
    }

    fn auth_headers(&self, url: &url::Url) -> HeaderMap {
        let mut headers = HeaderMap::new();
        if url.host_str() == Some(GITHUB_API_HOST) {
            headers.insert(
                header::ACCEPT,
                HeaderValue::from_static("application/vnd.github+json"),
            );
            headers.insert(
                "X-GitHub-Api-Version",
                HeaderValue::from_static("2022-11-28"),
            );
            if let Some(token) = self
                .github_token
                .read()
                .expect("token lock poisoned")
                .as_ref()
            {
                if let Ok(value) = HeaderValue::from_str(&format!("Bearer {token}")) {
                    headers.insert(header::AUTHORIZATION, value);
                }
            }
        }
        headers
    }

    fn cache_paths(&self, url: &str) -> (PathBuf, PathBuf) {
        let key = hex::encode(Sha256::digest(url.as_bytes()));
        (
            self.cache_dir.join(format!("{key}.meta.json")),
            self.cache_dir.join(format!("{key}.body")),
        )
    }

    fn read_cache(&self, url: &str) -> Option<(CacheMeta, Vec<u8>)> {
        let (meta_path, body_path) = self.cache_paths(url);
        let meta: CacheMeta = serde_json::from_slice(&std::fs::read(meta_path).ok()?).ok()?;
        if meta.url != url {
            return None;
        }
        let body = std::fs::read(body_path).ok()?;
        Some((meta, body))
    }

    fn write_cache(&self, meta: &CacheMeta, body: &[u8]) -> Result<()> {
        let (meta_path, body_path) = self.cache_paths(&meta.url);
        // Body first: a meta file never points at a missing body.
        fsutil::write_atomic(&body_path, body)?;
        fsutil::write_json(&meta_path, meta)
    }

    /// Cached body for `url`, if any, without touching the network.
    pub fn cached(&self, url: &str) -> Option<Fetched> {
        self.read_cache(url).map(|(meta, body)| Fetched {
            body,
            source: FetchSource::StaleCache,
            fetched_at: meta.fetched_at,
        })
    }

    /// GET with ETag cache. With `allow_stale`, a network failure (not an
    /// HTTP error status) falls back to the cached body.
    pub async fn get_cached(&self, url: &str, allow_stale: bool) -> Result<Fetched> {
        let parsed = url::Url::parse(url).map_err(|e| AppError::InvalidInput {
            message: format!("invalid URL {url}: {e}"),
        })?;
        let cached = self.read_cache(url);

        let mut request = self
            .client
            .get(parsed.clone())
            .headers(self.auth_headers(&parsed));
        if let Some((meta, _)) = &cached {
            if let Some(etag) = &meta.etag {
                request = request.header(header::IF_NONE_MATCH, etag);
            }
            if let Some(lm) = &meta.last_modified {
                request = request.header(header::IF_MODIFIED_SINCE, lm);
            }
        }

        let response = match request.send().await {
            Ok(r) => r,
            Err(err) => {
                if allow_stale {
                    if let Some((meta, body)) = cached {
                        tracing::warn!(%url, %err, "network failed, using cached copy");
                        return Ok(Fetched {
                            body,
                            source: FetchSource::StaleCache,
                            fetched_at: meta.fetched_at,
                        });
                    }
                }
                return Err(AppError::network(err));
            }
        };

        let status = response.status();
        if status == StatusCode::NOT_MODIFIED {
            if let Some((mut meta, body)) = cached {
                meta.fetched_at = Utc::now();
                let _ = self.write_cache(&meta, &body);
                return Ok(Fetched {
                    body,
                    source: FetchSource::NotModified,
                    fetched_at: meta.fetched_at,
                });
            }
            // 304 without a cache entry: the cache was wiped mid-flight.
            return Err(AppError::Http {
                status: status.as_u16(),
                url: url.into(),
            });
        }
        check_status(&response)?;

        let etag = header_string(response.headers(), header::ETAG);
        let last_modified = header_string(response.headers(), header::LAST_MODIFIED);
        let body = response.bytes().await.map_err(AppError::network)?.to_vec();
        let meta = CacheMeta {
            url: url.to_owned(),
            etag,
            last_modified,
            fetched_at: Utc::now(),
        };
        if meta.etag.is_some() || meta.last_modified.is_some() {
            std::fs::create_dir_all(&self.cache_dir).at(&self.cache_dir)?;
            if let Err(err) = self.write_cache(&meta, &body) {
                tracing::warn!(%url, %err, "could not write HTTP cache");
            }
        }
        Ok(Fetched {
            body,
            source: FetchSource::Network,
            fetched_at: meta.fetched_at,
        })
    }
}

fn header_string(headers: &HeaderMap, name: header::HeaderName) -> Option<String> {
    headers.get(name)?.to_str().ok().map(str::to_owned)
}

/// Map error statuses to `AppError`, detecting GitHub rate limits.
pub fn check_status(response: &reqwest::Response) -> Result<()> {
    let status = response.status();
    if status.is_success() {
        return Ok(());
    }
    let headers = response.headers();
    let remaining = header_string(
        headers,
        header::HeaderName::from_static("x-ratelimit-remaining"),
    );
    let reset = header_string(
        headers,
        header::HeaderName::from_static("x-ratelimit-reset"),
    )
    .and_then(|v| v.parse::<i64>().ok());
    let limited = matches!(
        status,
        StatusCode::FORBIDDEN | StatusCode::TOO_MANY_REQUESTS
    ) && remaining.as_deref() == Some("0");
    if limited {
        return Err(AppError::RateLimited {
            reset_at: reset.unwrap_or_else(|| Utc::now().timestamp() + 60),
        });
    }
    if status == StatusCode::NOT_FOUND {
        return Err(AppError::NotFound {
            what: response.url().to_string(),
        });
    }
    Err(AppError::Http {
        status: status.as_u16(),
        url: response.url().to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn etag_cache_sends_if_none_match_and_serves_304() {
        let server = MockServer::start().await;
        let dir = tempfile::tempdir().unwrap();
        let http = Http::new(dir.path()).unwrap();
        let url = format!("{}/catalog.json", server.uri());

        // Conditional request: registered first so it wins when the header matches.
        Mock::given(method("GET"))
            .and(path("/catalog.json"))
            .and(header("if-none-match", "\"v1\""))
            .respond_with(ResponseTemplate::new(304))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/catalog.json"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("ETag", "\"v1\"")
                    .set_body_string("hello"),
            )
            .expect(1)
            .mount(&server)
            .await;

        let first = http.get_cached(&url, false).await.unwrap();
        assert_eq!(first.source, FetchSource::Network);
        assert_eq!(first.body, b"hello");

        let second = http.get_cached(&url, false).await.unwrap();
        assert_eq!(second.source, FetchSource::NotModified);
        assert_eq!(second.body, b"hello");
    }

    #[tokio::test]
    async fn stale_cache_on_network_failure() {
        let server = MockServer::builder().start().await; // not pooled: drop stops it
        let dir = tempfile::tempdir().unwrap();
        let http = Http::new(dir.path()).unwrap();
        let url = format!("{}/c.json", server.uri());
        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("ETag", "x")
                    .set_body_string("cached"),
            )
            .mount(&server)
            .await;
        http.get_cached(&url, true).await.unwrap();
        drop(server); // server gone: connection refused

        let stale = http.get_cached(&url, true).await.unwrap();
        assert_eq!(stale.source, FetchSource::StaleCache);
        assert_eq!(stale.body, b"cached");
        assert!(http.get_cached(&url, false).await.is_err());
    }

    #[tokio::test]
    async fn rate_limit_is_reported_with_reset_time() {
        let server = MockServer::start().await;
        let dir = tempfile::tempdir().unwrap();
        let http = Http::new(dir.path()).unwrap();
        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(403)
                    .insert_header("x-ratelimit-remaining", "0")
                    .insert_header("x-ratelimit-reset", "1700000000"),
            )
            .mount(&server)
            .await;
        let err = http.get_cached(&server.uri(), false).await.unwrap_err();
        assert!(matches!(
            err,
            AppError::RateLimited {
                reset_at: 1_700_000_000
            }
        ));
    }

    #[test]
    fn token_only_sent_to_github_api() {
        let dir = tempfile::tempdir().unwrap();
        let http = Http::new(dir.path()).unwrap();
        http.set_github_token(Some("secret".into()));
        let gh = http.auth_headers(&url::Url::parse("https://api.github.com/repos/a/b").unwrap());
        assert!(gh.contains_key(header::AUTHORIZATION));
        let other =
            http.auth_headers(&url::Url::parse("https://example.com/catalog.json").unwrap());
        assert!(!other.contains_key(header::AUTHORIZATION));
    }
}
