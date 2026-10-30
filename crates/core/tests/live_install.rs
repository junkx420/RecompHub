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
use recomphub_core::update::{self, UpdateRequest};

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

/// Installs an older Zelda64Recomp release, adds a save, updates to the
/// latest release, then restores the old one.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "downloads two real releases from GitHub"]
async fn updates_real_release_and_restores() {
    let (entries, _) = catalog::parse(EMBEDDED_CATALOG.as_bytes(), Default::default()).unwrap();
    let latest_entry = entries
        .iter()
        .find(|e| e.id == "zelda64recomp")
        .unwrap()
        .clone();
    let mut old_entry = latest_entry.clone();
    old_entry.release_tag = Some("v1.2.0".into());

    let dir = tempfile::tempdir().unwrap();
    let dirs = AppDirs::at(dir.path().join("RecompHub"));
    dirs.ensure().unwrap();
    let http = Http::new(dirs.cache()).unwrap();
    http.set_github_token(std::env::var("GITHUB_TOKEN").ok());
    let quiet = |_: Progress| {};

    let old_release = github::fetch_release(&http, &old_entry).await.unwrap();
    let installed = install::install(InstallRequest {
        http: &http,
        lib: &dirs.library(None),
        installed_file: &dirs.installed_file(),
        entry: &old_entry,
        release: &old_release,
        options: InstallOptions::default(),
        cancel: &CancelFlag::new(),
        progress: &quiet,
    })
    .await
    .unwrap();
    let save = installed.dir.join("saves").join("slot0.bin");
    std::fs::create_dir_all(save.parent().unwrap()).unwrap();
    std::fs::write(&save, b"my progress").unwrap();
    std::fs::write(installed.dir.join("portable.txt"), b"").unwrap();

    let latest = github::fetch_release(&http, &latest_entry).await.unwrap();
    let updated = update::update(UpdateRequest {
        http: &http,
        dirs: &dirs,
        entry: &latest_entry,
        release: &latest,
        cancel: &CancelFlag::new(),
        progress: &quiet,
    })
    .await
    .unwrap();
    println!(
        "updated {} -> {}: {} files placed",
        installed.version,
        updated.version,
        updated.files.len()
    );
    assert_eq!(updated.version, latest.tag_name);
    assert_eq!(std::fs::read(&save).unwrap(), b"my progress");
    assert!(installed.dir.join("portable.txt").exists());
    assert!(updated.dir.join(&updated.exe).is_file());

    let restored =
        update::restore_previous(&dirs, &latest_entry, &CancelFlag::new(), &quiet).unwrap();
    println!("restored {}", restored.version);
    assert_eq!(restored.version, "v1.2.0");
    assert_eq!(std::fs::read(&save).unwrap(), b"my progress");
    let exe_hash =
        recomphub_core::download::sha256_file_blocking(&restored.dir.join(&restored.exe)).unwrap();
    assert_eq!(Some(&exe_hash), installed.files.get(&installed.exe));
}
