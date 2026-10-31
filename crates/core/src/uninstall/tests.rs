use super::*;
use crate::archive::tests::make_zip;
use crate::catalog::{self, EMBEDDED_CATALOG};
use crate::github::{Asset, Release};
use crate::http::Http;
use crate::install::{self, InstallOptions, InstallRequest};
use crate::job::{CancelFlag, Progress};
use chrono::Utc;
use sha2::{Digest, Sha256};
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

fn no_progress(_: Progress) {}

fn entry(user_data: &Path) -> CatalogEntry {
    let (entries, _) = catalog::parse(EMBEDDED_CATALOG.as_bytes(), Default::default()).unwrap();
    let mut entry = entries
        .into_iter()
        .find(|e| e.id == "zelda64recomp")
        .unwrap();
    entry.asset_pattern = r"(?i)-Windows\.zip$".into();
    entry.exe = "game.exe".into();
    entry.preserve = vec!["config.json".into()];
    entry.user_data_paths = vec![format!("%TEST%\\{}", user_data.display())];
    entry
}

/// Resolves the test's `%TEST%\<absolute path>` back to the path.
fn expand(path: &str) -> Option<PathBuf> {
    path.strip_prefix("%TEST%\\").map(PathBuf::from)
}

struct Fixture {
    _dir: tempfile::TempDir,
    dirs: AppDirs,
    http: Http,
    server: MockServer,
    user_data: PathBuf,
}

impl Fixture {
    async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let dirs = AppDirs::at(dir.path().join("RecompHub"));
        dirs.ensure().unwrap();
        let http = Http::new(dirs.cache()).unwrap();
        let user_data = dir.path().join("Roaming").join("Zelda64Recompiled");
        std::fs::create_dir_all(&user_data).unwrap();
        std::fs::write(user_data.join("global.sav"), "outside save").unwrap();
        let server = MockServer::start().await;
        let data = make_zip(&[
            ("game.exe", Some(b"MZ")),
            ("config.json", Some(b"default config")),
            ("assets/a.bin", Some(b"a")),
        ]);
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(data))
            .mount(&server)
            .await;
        Self {
            _dir: dir,
            dirs,
            http,
            server,
            user_data,
        }
    }

    async fn install(&self) -> InstalledApp {
        let data = make_zip(&[
            ("game.exe", Some(b"MZ")),
            ("config.json", Some(b"default config")),
            ("assets/a.bin", Some(b"a")),
        ]);
        let release = Release {
            tag_name: "v1".into(),
            name: None,
            body: None,
            prerelease: false,
            draft: false,
            published_at: None,
            html_url: "https://github.com/o/r".into(),
            assets: vec![Asset {
                id: 1,
                name: "Game-Windows.zip".into(),
                size: data.len() as u64,
                browser_download_url: format!("{}/a.zip", self.server.uri()),
                updated_at: Utc::now(),
                digest: Some(format!("sha256:{}", hex::encode(Sha256::digest(&data)))),
            }],
        };
        install::install(InstallRequest {
            http: &self.http,
            lib: &self.dirs.library(None),
            installed_file: &self.dirs.installed_file(),
            entry: &entry(&self.user_data),
            release: &release,
            options: InstallOptions::default(),
            cancel: &CancelFlag::new(),
            progress: &no_progress,
        })
        .await
        .unwrap()
    }

    fn uninstall(&self, keep_saves: bool) -> Result<(InstalledApp, UninstallOutcome)> {
        let entry = entry(&self.user_data);
        uninstall(UninstallRequest {
            dirs: &self.dirs,
            id: "zelda64recomp",
            entry: Some(&entry),
            keep_saves,
            expand: &expand,
        })
    }
}

fn write(dir: &Path, rel: &str, text: &str) {
    let path = dir.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn keep_saves_then_reinstall_brings_them_back() {
    let f = Fixture::new().await;
    let app = f.install().await;
    write(&app.dir, "saves/slot1.sav", "progress");
    write(&app.dir, "config.json", "my config");

    let (_, outcome) = f.uninstall(true).unwrap();
    assert!(!app.dir.exists());
    let saves = outcome.kept_saves.clone().unwrap();
    assert_eq!(
        std::fs::read_to_string(saves.join("saves/slot1.sav")).unwrap(),
        "progress"
    );
    assert_eq!(
        std::fs::read_to_string(saves.join("config.json")).unwrap(),
        "my config"
    );
    assert!(
        !saves.join("game.exe").exists(),
        "release files are not kept"
    );
    assert!(
        f.user_data.join("global.sav").exists(),
        "outside data untouched"
    );
    assert!(InstalledDb::load(&f.dirs.installed_file())
        .unwrap()
        .apps
        .is_empty());
    // Only the downloads folder is left in staging.
    let staging: Vec<_> = std::fs::read_dir(&f.dirs.library(None).staging)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(staging, ["downloads"]);

    let again = f.install().await;
    assert_eq!(
        std::fs::read_to_string(again.dir.join("saves/slot1.sav")).unwrap(),
        "progress"
    );
    // The release ships config.json, so it wins; the user's copy stays in the backup.
    assert_eq!(
        std::fs::read_to_string(again.dir.join("config.json")).unwrap(),
        "default config"
    );
    assert_eq!(
        std::fs::read_to_string(saves.join("config.json")).unwrap(),
        "my config"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn without_saves_everything_goes() {
    let f = Fixture::new().await;
    let app = f.install().await;
    write(&app.dir, "saves/slot1.sav", "progress");

    let (_, outcome) = f.uninstall(false).unwrap();
    assert_eq!(outcome.kept_saves, None);
    assert_eq!(
        outcome.removed_user_data,
        std::slice::from_ref(&f.user_data)
    );
    assert!(!app.dir.exists());
    assert!(!f.user_data.exists());
    assert!(!f.dirs.library(None).backups.join("zelda64recomp").exists());
}

#[tokio::test(flavor = "multi_thread")]
async fn not_installed_is_an_error() {
    let f = Fixture::new().await;
    assert!(matches!(
        f.uninstall(true),
        Err(AppError::NotInstalled { .. })
    ));
}

#[test]
fn expands_only_known_roots() {
    let local = std::env::var_os("LOCALAPPDATA").or_else(|| std::env::var_os("HOME"));
    if std::env::var_os("LOCALAPPDATA").is_some() {
        assert_eq!(
            expand_user_path("%LOCALAPPDATA%\\Zelda64Recompiled"),
            Some(PathBuf::from(local.unwrap()).join("Zelda64Recompiled"))
        );
    }
    assert_eq!(expand_user_path("%LOCALAPPDATA%\\"), None);
    assert_eq!(expand_user_path("%LOCALAPPDATA%\\..\\x"), None);
    assert_eq!(expand_user_path("C:\\Windows"), None);
    assert_eq!(expand_user_path("%TEMP%\\x"), None);
}
