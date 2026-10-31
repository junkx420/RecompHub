//! Fresh installs: download, verify, unpack to staging, move into the library.
//!
//! The game folder only appears once everything is unpacked and checked;
//! the final step is a rename within one volume (see `paths`). Content that
//! RecompHub did not install is never overwritten: it is reported by
//! [`preflight`], and with the user's consent moved to the backups folder.

use crate::archive;
use crate::catalog::CatalogEntry;
use crate::download::{self, DownloadSpec};
use crate::error::{AppError, IoContext, Result};
use crate::github::Release;
use crate::http::Http;
use crate::installed::{InstalledApp, InstalledDb};
use crate::job::{CancelFlag, Progress, ProgressFn};
use crate::paths::LibraryDirs;
use chrono::Utc;
use globset::GlobBuilder;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, Deserialize)]
pub struct InstallOptions {
    /// The user agreed to move existing foreign content aside.
    #[serde(default)]
    pub replace_foreign: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Preflight {
    /// Nothing in the way.
    Ready { target: PathBuf },
    /// Already installed by RecompHub.
    Installed { version: String, target: PathBuf },
    /// The target folder exists and has files RecompHub does not know.
    ForeignContent { target: PathBuf },
}

/// Where `entry` would be installed and what is in the way.
pub fn preflight(lib: &LibraryDirs, db: &InstalledDb, entry: &CatalogEntry) -> Result<Preflight> {
    if let Some(app) = db.get(&entry.id) {
        return Ok(Preflight::Installed {
            version: app.version.clone(),
            target: crate::paths::normalize(&app.dir),
        });
    }
    let target = lib.app(&entry.id);
    let occupied = match std::fs::read_dir(&target) {
        Ok(mut entries) => entries.next().is_some(),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => false,
        Err(err) => return Err(AppError::io(&target, err)),
    };
    let target = crate::paths::normalize(&target);
    Ok(if occupied {
        Preflight::ForeignContent { target }
    } else {
        Preflight::Ready { target }
    })
}

/// Find the executable for `pattern` among `/`-separated relative `files`.
/// A pattern without a folder also matches in subfolders (shallowest wins),
/// for releases that keep the binary one level down.
pub fn resolve_exe(pattern: &str, files: &[String]) -> Result<String> {
    let pattern = pattern.replace('\\', "/");
    let not_found = || AppError::ExeNotFound {
        pattern: pattern.clone(),
    };
    let glob = GlobBuilder::new(&pattern)
        .literal_separator(true)
        .case_insensitive(true)
        .build()
        .map_err(|_| not_found())?
        .compile_matcher();
    let mut matches: Vec<&String> = files.iter().filter(|f| glob.is_match(f.as_str())).collect();
    if matches.is_empty() && !pattern.contains('/') {
        matches = files
            .iter()
            .filter(|f| glob.is_match(f.rsplit('/').next().unwrap_or(f)))
            .collect();
    }
    matches.sort_by_key(|f| (f.matches('/').count(), f.to_lowercase()));
    matches.first().map(|f| (*f).clone()).ok_or_else(not_found)
}

/// Sortable, unique-enough timestamp for folder names and revisions.
pub(crate) fn stamp() -> String {
    Utc::now().format("%Y%m%d-%H%M%S%6f").to_string()
}

pub(crate) async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T> + Send + 'static,
) -> Result<T> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| AppError::InvalidInput {
            message: format!("background task failed: {e}"),
        })?
}

pub struct InstallRequest<'a> {
    pub http: &'a Http,
    pub lib: &'a LibraryDirs,
    pub installed_file: &'a Path,
    pub entry: &'a CatalogEntry,
    pub release: &'a Release,
    pub options: InstallOptions,
    pub cancel: &'a CancelFlag,
    pub progress: ProgressFn<'a>,
}

/// Install `release` of `entry` into the library and record it.
pub async fn install(req: InstallRequest<'_>) -> Result<InstalledApp> {
    let InstallRequest {
        http,
        lib,
        installed_file,
        entry,
        release,
        options,
        cancel,
        progress,
    } = req;

    let db = InstalledDb::load(installed_file)?;
    let foreign = match preflight(lib, &db, entry)? {
        Preflight::Installed { .. } => {
            return Err(AppError::AlreadyInstalled {
                id: entry.id.clone(),
            })
        }
        Preflight::ForeignContent { target } if !options.replace_foreign => {
            return Err(AppError::ForeignContent { path: target })
        }
        Preflight::ForeignContent { .. } => true,
        Preflight::Ready { .. } => false,
    };

    let prepared = prepare(http, lib, entry, release, cancel, progress).await?;
    let placed = place(&prepared, lib, entry, foreign).await;
    // The work folder is only scaffolding; never leave it behind.
    let _ = tokio::fs::remove_dir_all(&prepared.work).await;
    let target = placed?;
    // Saves kept by an earlier uninstall come back with a reinstall.
    match tokio::task::block_in_place(|| {
        crate::uninstall::restore_kept_saves(lib, &entry.id, &target)
    }) {
        Ok(0) => {}
        Ok(count) => tracing::info!(id = %entry.id, count, "restored kept saves"),
        Err(err) => tracing::warn!(id = %entry.id, %err, "could not restore kept saves"),
    }

    progress(Progress::Finalizing);
    let app = InstalledApp {
        id: entry.id.clone(),
        name: entry.name.clone(),
        version: release.tag_name.clone(),
        asset_name: prepared.asset_name,
        asset_id: prepared.asset_id,
        asset_sha256: prepared.asset_sha256,
        dir: target.clone(),
        exe: prepared.exe,
        installed_at: Utc::now(),
        updated_at: None,
        files: prepared.files,
        previous: None,
        revision: stamp(),
        shortcuts: Vec::new(),
        steam_users: Vec::new(),
    };
    let mut db = InstalledDb::load(installed_file)?;
    db.apps.insert(app.id.clone(), app.clone());
    if let Err(err) = db.save(installed_file) {
        // Without a record the folder would look foreign; undo the move.
        let _ = tokio::fs::remove_dir_all(&target).await;
        return Err(err);
    }
    tracing::info!(id = %app.id, version = %app.version, dir = ?app.dir, "installed");
    Ok(app)
}

/// A release downloaded, verified and unpacked into a staging work folder.
/// The caller removes `work` when done with it.
pub(crate) struct Prepared {
    pub work: PathBuf,
    /// Folder holding the release files (inside `work`).
    pub root: PathBuf,
    pub exe: String,
    pub files: BTreeMap<String, String>,
    pub asset_name: String,
    pub asset_id: u64,
    pub asset_sha256: Option<String>,
}

/// Download, verify and unpack `release` into a fresh work folder in the
/// library's staging area. Cleans up after itself on failure.
pub(crate) async fn prepare(
    http: &Http,
    lib: &LibraryDirs,
    entry: &CatalogEntry,
    release: &Release,
    cancel: &CancelFlag,
    progress: ProgressFn<'_>,
) -> Result<Prepared> {
    let asset = release.asset_for(entry)?;
    let spec = DownloadSpec::for_asset(asset);
    let downloads = lib.staging.join("downloads");
    let archive_path = download::download(http, &spec, &downloads, cancel, progress).await?;
    let work = lib.staging.join(format!("{}-{}", entry.id, stamp()));
    let result = unpack(&archive_path, &asset.name, &work, entry, cancel, progress).await;
    // A finished download is never resumed, so it is only scaffolding.
    let _ = tokio::fs::remove_file(&archive_path).await;
    match result {
        Ok((root, exe, files)) => Ok(Prepared {
            work,
            root,
            exe,
            files,
            asset_name: asset.name.clone(),
            asset_id: asset.id,
            asset_sha256: spec.sha256,
        }),
        Err(err) => {
            let _ = tokio::fs::remove_dir_all(&work).await;
            Err(err)
        }
    }
}

async fn unpack(
    archive_path: &Path,
    asset_name: &str,
    work: &Path,
    entry: &CatalogEntry,
    cancel: &CancelFlag,
    progress: ProgressFn<'_>,
) -> Result<(PathBuf, String, BTreeMap<String, String>)> {
    cancel.check()?;
    let unpacked = work.join("unpacked");
    // The progress callback is borrowed and cannot move into spawn_blocking;
    // block_in_place needs the multi-threaded runtime, which Tauri uses.
    tokio::task::block_in_place(|| {
        archive::extract(archive_path, asset_name, &unpacked, cancel, progress)
    })?;
    cancel.check()?;

    let root = archive::content_root(&unpacked)?;
    let (files, hashes) = {
        let root = root.clone();
        blocking(move || {
            let files = archive::list_files(&root)?;
            let hashes = hash_files(&root, &files)?;
            Ok((files, hashes))
        })
        .await?
    };
    let exe = resolve_exe(&entry.exe, &files)?;
    cancel.check()?;
    Ok((root, exe, hashes))
}

/// SHA-256 of each relative file below `root`. Blocking.
pub(crate) fn hash_files(root: &Path, files: &[String]) -> Result<BTreeMap<String, String>> {
    files
        .iter()
        .map(|rel| {
            Ok((
                rel.clone(),
                download::sha256_file_blocking(&root.join(rel))?,
            ))
        })
        .collect()
}

/// Move a prepared release into the library as a new game folder.
async fn place(
    prepared: &Prepared,
    lib: &LibraryDirs,
    entry: &CatalogEntry,
    foreign: bool,
) -> Result<PathBuf> {
    let root = &prepared.root;
    let target = lib.app(&entry.id);
    tokio::fs::create_dir_all(&lib.apps).await.at(&lib.apps)?;
    if foreign {
        let backup = lib
            .backups
            .join(format!("{}-foreign-{}", entry.id, stamp()));
        tokio::fs::create_dir_all(&lib.backups)
            .await
            .at(&lib.backups)?;
        tokio::fs::rename(&target, &backup).await.at(&backup)?;
        tracing::warn!(from = ?target, to = ?backup, "moved foreign content aside");
    } else if target.exists() {
        // Preflight saw it empty; an empty folder can simply go.
        tokio::fs::remove_dir(&target).await.at(&target)?;
    }
    tokio::fs::rename(root, &target).await.at(&target)?;
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::archive::tests::make_zip;
    use crate::catalog::{self, EMBEDDED_CATALOG};
    use crate::github::Asset;
    use crate::paths::AppDirs;
    use sha2::{Digest, Sha256};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn no_progress(_: Progress) {}

    fn entry() -> CatalogEntry {
        let (entries, _) = catalog::parse(EMBEDDED_CATALOG.as_bytes(), Default::default()).unwrap();
        let mut entry = entries
            .into_iter()
            .find(|e| e.id == "zelda64recomp")
            .unwrap();
        entry.asset_pattern = r"(?i)-Windows\.zip$".into();
        entry
    }

    fn release(url: String, data: &[u8]) -> Release {
        Release {
            tag_name: "v1.2.0".into(),
            name: None,
            body: None,
            prerelease: false,
            draft: false,
            published_at: None,
            html_url: "https://github.com/o/r/releases/tag/v1.2.0".into(),
            assets: vec![Asset {
                id: 7,
                name: "Zelda64Recompiled-v1.2.0-Windows.zip".into(),
                size: data.len() as u64,
                browser_download_url: url,
                updated_at: Utc::now(),
                digest: Some(format!("sha256:{}", hex::encode(Sha256::digest(data)))),
            }],
        }
    }

    async fn serve(data: Vec<u8>) -> MockServer {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/asset.zip"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(data))
            .mount(&server)
            .await;
        server
    }

    struct Fixture {
        _dir: tempfile::TempDir,
        dirs: AppDirs,
        lib: LibraryDirs,
        http: Http,
    }

    fn fixture() -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let dirs = AppDirs::at(dir.path().join("RecompHub"));
        dirs.ensure().unwrap();
        let lib = dirs.library(None);
        let http = Http::new(dirs.cache()).unwrap();
        Fixture {
            _dir: dir,
            dirs,
            lib,
            http,
        }
    }

    async fn run(
        f: &Fixture,
        entry: &CatalogEntry,
        release: &Release,
        options: InstallOptions,
    ) -> Result<InstalledApp> {
        install(InstallRequest {
            http: &f.http,
            lib: &f.lib,
            installed_file: &f.dirs.installed_file(),
            entry,
            release,
            options,
            cancel: &CancelFlag::new(),
            progress: &no_progress,
        })
        .await
    }

    fn game_zip() -> Vec<u8> {
        make_zip(&[
            ("Zelda64Recompiled/", None),
            ("Zelda64Recompiled/Zelda64Recompiled.exe", Some(b"MZgame")),
            ("Zelda64Recompiled/assets/a.bin", Some(b"asset")),
        ])
    }

    #[test]
    fn resolve_exe_prefers_exact_then_shallowest() {
        let files: Vec<String> = ["bin/game.exe", "game.exe", "tools/x/game.exe"]
            .map(String::from)
            .to_vec();
        assert_eq!(resolve_exe("game.exe", &files).unwrap(), "game.exe");
        let nested: Vec<String> = ["b/deep/Game.EXE", "a/game.exe"].map(String::from).to_vec();
        assert_eq!(resolve_exe("game.exe", &nested).unwrap(), "a/game.exe");
        assert_eq!(resolve_exe("bin\\*.exe", &files).unwrap(), "bin/game.exe");
        assert!(matches!(
            resolve_exe("missing.exe", &files),
            Err(AppError::ExeNotFound { .. })
        ));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn installs_end_to_end() {
        let f = fixture();
        let data = game_zip();
        let server = serve(data.clone()).await;
        let entry = entry();
        let release = release(format!("{}/asset.zip", server.uri()), &data);

        let app = run(&f, &entry, &release, InstallOptions::default())
            .await
            .unwrap();
        let target = f.lib.app("zelda64recomp");
        assert_eq!(app.dir, target);
        assert_eq!(app.exe, "Zelda64Recompiled.exe");
        assert_eq!(app.version, "v1.2.0");
        assert_eq!(
            std::fs::read(target.join("Zelda64Recompiled.exe")).unwrap(),
            b"MZgame"
        );
        assert_eq!(
            app.files.get("assets/a.bin").map(String::as_str),
            Some(hex::encode(Sha256::digest(b"asset")).as_str())
        );

        let db = InstalledDb::load(&f.dirs.installed_file()).unwrap();
        assert_eq!(db.get("zelda64recomp"), Some(&app));
        // Staging is clean: no work folders and no leftover download.
        let leftovers: Vec<_> = std::fs::read_dir(&f.lib.staging)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(leftovers, ["downloads"]);
        assert_eq!(
            std::fs::read_dir(f.lib.staging.join("downloads"))
                .unwrap()
                .count(),
            0
        );

        assert!(matches!(
            preflight(&f.lib, &db, &entry).unwrap(),
            Preflight::Installed { .. }
        ));
        assert!(matches!(
            run(&f, &entry, &release, InstallOptions::default()).await,
            Err(AppError::AlreadyInstalled { .. })
        ));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn foreign_content_needs_consent_and_is_backed_up() {
        let f = fixture();
        let data = game_zip();
        let server = serve(data.clone()).await;
        let entry = entry();
        let release = release(format!("{}/asset.zip", server.uri()), &data);
        let target = f.lib.app("zelda64recomp");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("mine.sav"), b"precious").unwrap();

        let db = InstalledDb::default();
        assert!(matches!(
            preflight(&f.lib, &db, &entry).unwrap(),
            Preflight::ForeignContent { .. }
        ));
        let err = run(&f, &entry, &release, InstallOptions::default())
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::ForeignContent { .. }));
        assert_eq!(std::fs::read(target.join("mine.sav")).unwrap(), b"precious");

        run(
            &f,
            &entry,
            &release,
            InstallOptions {
                replace_foreign: true,
            },
        )
        .await
        .unwrap();
        assert!(!target.join("mine.sav").exists());
        let backup = std::fs::read_dir(&f.lib.backups)
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        assert_eq!(std::fs::read(backup.join("mine.sav")).unwrap(), b"precious");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn missing_exe_leaves_no_trace() {
        let f = fixture();
        let data = make_zip(&[("readme.txt", Some(b"hi"))]);
        let server = serve(data.clone()).await;
        let entry = entry();
        let release = release(format!("{}/asset.zip", server.uri()), &data);

        let err = run(&f, &entry, &release, InstallOptions::default())
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::ExeNotFound { .. }));
        assert!(!f.lib.app("zelda64recomp").exists());
        assert!(InstalledDb::load(&f.dirs.installed_file())
            .unwrap()
            .apps
            .is_empty());
        let leftovers: Vec<_> = std::fs::read_dir(&f.lib.staging)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(leftovers, ["downloads"]);
        assert_eq!(
            std::fs::read_dir(f.lib.staging.join("downloads"))
                .unwrap()
                .count(),
            0
        );
    }
}
