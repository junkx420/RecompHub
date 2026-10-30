//! Catalog of installable ports.
//!
//! Load order: remote URL (ETag-cached), then the cached copy, then the copy
//! embedded in the binary. Entries are validated one by one; a broken entry is
//! dropped with a warning instead of rejecting the whole catalog.

mod types;

pub use types::*;

use crate::error::{AppError, Result};
use crate::http::{FetchSource, Http};
use chrono::{DateTime, Utc};
use regex::Regex;
use serde::Serialize;
use std::collections::HashSet;

pub const SCHEMA_VERSION: u32 = 1;

/// Built-in catalog, used when neither network nor cache is available.
pub const EMBEDDED_CATALOG: &str = include_str!("../../../../catalog/catalog.json");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CatalogOrigin {
    Remote,
    Cache,
    Embedded,
}

#[derive(Debug, Clone, Serialize)]
pub struct LoadedCatalog {
    pub entries: Vec<CatalogEntry>,
    pub origin: CatalogOrigin,
    pub fetched_at: Option<DateTime<Utc>>,
    /// Non-fatal problems: dropped entries, fallback reasons.
    pub warnings: Vec<String>,
}

impl LoadedCatalog {
    pub fn get(&self, id: &str) -> Option<&CatalogEntry> {
        self.entries.iter().find(|e| e.id == id)
    }
}

/// Parse and validate catalog JSON. Fails only if the document as a whole is
/// unusable; invalid entries end up in the returned warnings.
pub fn parse(json: &[u8], source: EntrySource) -> Result<(Vec<CatalogEntry>, Vec<String>)> {
    let doc: CatalogDocument =
        serde_json::from_slice(json).map_err(|e| AppError::CatalogInvalid {
            errors: vec![e.to_string()],
        })?;
    if doc.schema_version > SCHEMA_VERSION {
        return Err(AppError::CatalogInvalid {
            errors: vec![format!(
                "schema_version {} is newer than supported {SCHEMA_VERSION}; update RecompHub",
                doc.schema_version
            )],
        });
    }

    let mut entries = Vec::new();
    let mut warnings = Vec::new();
    let mut seen = HashSet::new();
    for (index, raw) in doc.entries.into_iter().enumerate() {
        let label = raw
            .get("id")
            .and_then(|v| v.as_str())
            .map_or_else(|| format!("#{index}"), str::to_owned);
        let mut entry: CatalogEntry = match serde_json::from_value(raw) {
            Ok(e) => e,
            Err(err) => {
                warnings.push(format!("entry {label}: {err}"));
                continue;
            }
        };
        let problems = entry.validate();
        if !problems.is_empty() {
            warnings.push(format!("entry {label}: {}", problems.join("; ")));
            continue;
        }
        if !seen.insert(entry.id.clone()) {
            warnings.push(format!("entry {label}: duplicate id"));
            continue;
        }
        entry.source = source.clone();
        entries.push(entry);
    }
    Ok((entries, warnings))
}

/// Load the official catalog from `url`, falling back to cache, then to the
/// embedded copy. Never fails: the embedded copy is validated by tests.
pub async fn load(http: &Http, url: &str) -> LoadedCatalog {
    let mut warnings = Vec::new();
    match http.get_cached(url, true).await {
        Ok(fetched) => match parse(&fetched.body, EntrySource::Official) {
            Ok((entries, mut w)) if !entries.is_empty() => {
                warnings.append(&mut w);
                let origin = match fetched.source {
                    FetchSource::Network | FetchSource::NotModified => CatalogOrigin::Remote,
                    FetchSource::StaleCache => CatalogOrigin::Cache,
                };
                return LoadedCatalog {
                    entries,
                    origin,
                    fetched_at: Some(fetched.fetched_at),
                    warnings,
                };
            }
            Ok(_) => warnings.push(format!("catalog from {url} has no valid entries")),
            Err(err) => warnings.push(format!("catalog from {url} rejected: {err}")),
        },
        Err(err) => warnings.push(format!("catalog download failed: {err}")),
    }
    tracing::warn!(?warnings, "falling back to embedded catalog");
    embedded(warnings)
}

fn embedded(mut warnings: Vec<String>) -> LoadedCatalog {
    let (entries, mut w) = parse(EMBEDDED_CATALOG.as_bytes(), EntrySource::Official)
        .expect("embedded catalog is valid (checked by tests)");
    warnings.append(&mut w);
    LoadedCatalog {
        entries,
        origin: CatalogOrigin::Embedded,
        fetched_at: None,
        warnings,
    }
}

/// Pick the single release asset whose name matches `pattern`.
pub fn select_asset<'a>(pattern: &str, names: &[&'a str]) -> Result<&'a str> {
    let re = Regex::new(pattern).map_err(|e| AppError::InvalidInput {
        message: format!("asset_pattern: {e}"),
    })?;
    let matches: Vec<&str> = names.iter().copied().filter(|n| re.is_match(n)).collect();
    match matches.as_slice() {
        [one] => Ok(one),
        [] => Err(AppError::AssetNoMatch {
            pattern: pattern.into(),
            candidates: names.iter().map(|s| s.to_string()).collect(),
        }),
        _ => Err(AppError::AssetAmbiguous {
            pattern: pattern.into(),
            matches: matches.iter().map(|s| s.to_string()).collect(),
        }),
    }
}

#[cfg(test)]
mod tests;
