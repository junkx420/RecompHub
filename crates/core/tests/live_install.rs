//! Installs a real port from GitHub into a temporary folder.
//!
//! Needs network access and downloads tens of megabytes, so it is ignored by
//! default. Run it with:
//!
//! ```text
//! cargo test -p recomphub-core --test live_install -- --ignored --nocapture
//! ```
//!
//! `RECOMPHUB_LIVE_ID` picks another catalog entry (default `zelda64recomp`).

use recomphub_core::catalog::{self, EMBEDDED_CATALOG};
use recomphub_core::github;
use recomphub_core::http::Http;
use recomphub_core::install::{self, InstallOptions, InstallRequest};
use recomphub_core::installed::InstalledDb;
use recomphub_core::job::{CancelFlag, Progress};
use recomphub_core::paths::AppDirs;

#[tokio::test(flavor = "multi_thread")]
#[ignore = "downloads a real release from GitHub"]
async fn installs_real_release() {
    let id = std::env::var("RECOMPHUB_LIVE_ID").unwrap_or_else(|_| "zelda64recomp".into());
    let (entries, _) = catalog::parse(EMBEDDED_CATALOG.as_bytes(), Default::default()).unwrap();
    let entry = entries.iter().find(|e| e.id == id).expect("id in catalog");

    let dir = tempfile::tempdir().unwrap();
    let dirs = AppDirs::at(dir.path().join("RecompHub"));
    dirs.ensure().unwrap();
    let lib = dirs.library(None);
    let http = Http::new(dirs.cache()).unwrap();
    http.set_github_token(std::env::var("GITHUB_TOKEN").ok());

    let release = github::fetch_release(&http, entry).await.unwrap();
    let asset = release.asset_for(entry).unwrap();
    println!(
        "{} {}: {} ({} bytes, digest {:?})",
        entry.id, release.tag_name, asset.name, asset.size, asset.digest
    );

    let report = |p: Progress| {
        if !matches!(
            p,
            Progress::Downloading { .. } | Progress::Extracting { .. }
        ) {
            println!("{p:?}");
        }
    };
    let app = install::install(InstallRequest {
        http: &http,
        lib: &lib,
        installed_file: &dirs.installed_file(),
        entry,
        release: &release,
        options: InstallOptions::default(),
        cancel: &CancelFlag::new(),
        progress: &report,
    })
    .await
    .unwrap();

    println!("installed {} files, exe {}", app.files.len(), app.exe);
    assert!(app.dir.join(&app.exe).is_file());
    assert_eq!(
        InstalledDb::load(&dirs.installed_file()).unwrap().get(&id),
        Some(&app)
    );
}
