//! Check every catalog entry against the GitHub API: the release must exist
//! and `asset_pattern` must match exactly one of its assets.
//!
//! Usage: `cargo run -p recomphub-core --bin verify-catalog [catalog.json]`
//! Set `GITHUB_TOKEN` to avoid the anonymous rate limit. Prints a Markdown
//! table and exits with status 1 if any entry fails.

use recomphub_core::catalog::{self, EntrySource};
use recomphub_core::github;
use recomphub_core::http::Http;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "catalog/catalog.json".into());
    let json = std::fs::read(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));
    let (entries, warnings) =
        catalog::parse(&json, EntrySource::Official).unwrap_or_else(|e| panic!("{e}"));

    let cache = tempfile_dir();
    let http = Http::new(&cache).expect("http client");
    http.set_github_token(std::env::var("GITHUB_TOKEN").ok());

    let mut failed = !warnings.is_empty();
    for w in &warnings {
        println!("- invalid entry: {w}");
    }
    println!("| id | repo | release | asset | result |");
    println!("|---|---|---|---|---|");
    for entry in &entries {
        let (tag, asset, result) = match github::fetch_release(&http, entry).await {
            Ok(release) => match release.asset_for(entry) {
                Ok(asset) => (
                    release.tag_name.clone(),
                    asset.name.clone(),
                    "ok".to_owned(),
                ),
                Err(err) => {
                    failed = true;
                    let names: Vec<_> = release.assets.iter().map(|a| a.name.as_str()).collect();
                    (
                        release.tag_name.clone(),
                        "-".into(),
                        format!("FAIL: {err}; assets: {}", names.join(", ")),
                    )
                }
            },
            Err(err) => {
                failed = true;
                ("-".into(), "-".into(), format!("FAIL: {err}"))
            }
        };
        println!(
            "| {} | {} | {tag} | {asset} | {result} |",
            entry.id, entry.repo
        );
    }
    let _ = std::fs::remove_dir_all(&cache);
    if failed {
        std::process::exit(1);
    }
}

fn tempfile_dir() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("recomphub-verify-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}
