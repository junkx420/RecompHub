use super::*;
use crate::archive::tests::make_zip;
use crate::catalog::{self, EMBEDDED_CATALOG};
use crate::github::Asset;
use crate::install::{InstallOptions, InstallRequest};
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
    entry.exe = "game.exe".into();
    entry.preserve = vec!["config/*.json".into()];
    entry
}

struct Fixture {
    _dir: tempfile::TempDir,
    dirs: AppDirs,
    http: Http,
    server: MockServer,
}

impl Fixture {
    async fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let dirs = AppDirs::at(dir.path().join("RecompHub"));
        dirs.ensure().unwrap();
        let http = Http::new(dirs.cache()).unwrap();
        Self {
            _dir: dir,
            dirs,
            http,
            server: MockServer::start().await,
        }
    }

    fn app_dir(&self) -> PathBuf {
        self.dirs.default_library().join("zelda64recomp")
    }

    fn db(&self) -> InstalledDb {
        InstalledDb::load(&self.dirs.installed_file()).unwrap()
    }

    /// Serve `files` as the release `tag` and return it.
    async fn release(&self, tag: &str, files: &[(&str, &[u8])]) -> Release {
        let entries: Vec<(&str, Option<&[u8]>)> =
            files.iter().map(|(n, d)| (*n, Some(*d))).collect();
        let data = make_zip(&entries);
        let route = format!("/{tag}.zip");
        Mock::given(method("GET"))
            .and(path(route.as_str()))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(data.clone()))
            .mount(&self.server)
            .await;
        Release {
            tag_name: tag.into(),
            name: None,
            body: None,
            prerelease: false,
            published_at: None,
            html_url: "https://github.com/o/r".into(),
            assets: vec![Asset {
                id: 1,
                name: format!("Game-{tag}-Windows.zip"),
                size: data.len() as u64,
                browser_download_url: format!("{}{route}", self.server.uri()),
                updated_at: Utc::now(),
                digest: Some(format!("sha256:{}", hex::encode(Sha256::digest(&data)))),
            }],
        }
    }

    async fn install(&self, release: &Release) -> InstalledApp {
        install::install(InstallRequest {
            http: &self.http,
            lib: &self.dirs.library(None),
            installed_file: &self.dirs.installed_file(),
            entry: &entry(),
            release,
            options: InstallOptions::default(),
            cancel: &CancelFlag::new(),
            progress: &no_progress,
        })
        .await
        .unwrap()
    }

    async fn update(&self, release: &Release) -> Result<InstalledApp> {
        update(UpdateRequest {
            http: &self.http,
            dirs: &self.dirs,
            entry: &entry(),
            release,
            cancel: &CancelFlag::new(),
            progress: &no_progress,
        })
        .await
    }

    fn read(&self, rel: &str) -> Option<String> {
        std::fs::read_to_string(self.app_dir().join(rel)).ok()
    }

    fn write(&self, rel: &str, text: &str) {
        let path = self.app_dir().join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
}

const V1: &[(&str, &[u8])] = &[
    ("game.exe", b"exe v1"),
    ("data/a.bin", b"a v1"),
    ("data/old-only.bin", b"dropped in v2"),
    ("config/default.json", b"shipped config v1"),
];

const V2: &[(&str, &[u8])] = &[
    ("game.exe", b"exe v2"),
    ("data/a.bin", b"a v2"),
    ("data/new.bin", b"new in v2"),
    ("config/default.json", b"shipped config v2"),
    ("saves/slot1.sav", b"v2 ships a file where the user has one"),
];

#[tokio::test(flavor = "multi_thread")]
async fn update_replaces_release_files_and_keeps_user_data() {
    let f = Fixture::new().await;
    f.install(&f.release("v1", V1).await).await;
    // User and port data that appeared after the install.
    f.write("saves/slot1.sav", "my progress");
    f.write("config/default.json", "my tweaked config");
    f.write("portable.txt", "");

    let updated = f.update(&f.release("v2", V2).await).await.unwrap();

    assert_eq!(f.read("game.exe").unwrap(), "exe v2");
    assert_eq!(f.read("data/a.bin").unwrap(), "a v2");
    assert_eq!(f.read("data/new.bin").unwrap(), "new in v2");
    assert_eq!(f.read("data/old-only.bin"), None);
    // Preserved, user-created and colliding files are untouched.
    assert_eq!(f.read("config/default.json").unwrap(), "my tweaked config");
    assert_eq!(f.read("saves/slot1.sav").unwrap(), "my progress");
    assert_eq!(f.read("portable.txt").unwrap(), "");

    assert_eq!(updated.version, "v2");
    let manifest: Vec<_> = updated.files.keys().map(String::as_str).collect();
    assert_eq!(manifest, ["data/a.bin", "data/new.bin", "game.exe"]);
    let previous = updated.previous.clone().unwrap();
    assert_eq!(previous.version, "v1");
    assert_eq!(
        std::fs::read_to_string(previous.dir.join("files/data/old-only.bin")).unwrap(),
        "dropped in v2"
    );
    assert_eq!(f.db().get("zelda64recomp"), Some(&updated));

    // No journal or work folder left behind.
    let staging: Vec<_> = std::fs::read_dir(&f.dirs.library(None).staging)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(staging, ["downloads"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn same_version_is_up_to_date() {
    let f = Fixture::new().await;
    let v1 = f.release("v1", V1).await;
    f.install(&v1).await;
    assert!(matches!(
        f.update(&v1).await,
        Err(AppError::UpToDate { .. })
    ));
}

#[tokio::test(flavor = "multi_thread")]
async fn restore_swaps_back_and_forth_and_keeps_one_backup() {
    let f = Fixture::new().await;
    f.install(&f.release("v1", V1).await).await;
    f.write("saves/slot1.sav", "my progress");
    let v2 = f.update(&f.release("v2", V2).await).await.unwrap();
    let v2_backup = v2.previous.clone().unwrap().dir;

    let restored = restore_previous(&f.dirs, &entry(), &CancelFlag::new(), &no_progress).unwrap();
    assert_eq!(restored.version, "v1");
    assert_eq!(f.read("game.exe").unwrap(), "exe v1");
    assert_eq!(f.read("data/old-only.bin").unwrap(), "dropped in v2");
    assert_eq!(f.read("data/new.bin"), None);
    assert_eq!(f.read("saves/slot1.sav").unwrap(), "my progress");
    assert_eq!(restored.previous.as_ref().unwrap().version, "v2");
    assert!(!v2_backup.exists(), "used backup is removed");

    let again = restore_previous(&f.dirs, &entry(), &CancelFlag::new(), &no_progress).unwrap();
    assert_eq!(again.version, "v2");
    assert_eq!(f.read("game.exe").unwrap(), "exe v2");
    assert_eq!(f.read("saves/slot1.sav").unwrap(), "my progress");
    let backups = std::fs::read_dir(f.dirs.library(None).backups.join("zelda64recomp"))
        .unwrap()
        .count();
    assert_eq!(backups, 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn restore_without_backup_fails() {
    let f = Fixture::new().await;
    f.install(&f.release("v1", V1).await).await;
    assert!(matches!(
        restore_previous(&f.dirs, &entry(), &CancelFlag::new(), &no_progress),
        Err(AppError::NoPreviousVersion { .. })
    ));
}

/// A journal whose swap replaces `game.exe` and adds `new.bin`.
fn journal_fixture(root: &Path) -> Journal {
    let app_dir = root.join("app");
    let incoming = root.join("incoming");
    std::fs::create_dir_all(&app_dir).unwrap();
    std::fs::create_dir_all(&incoming).unwrap();
    std::fs::write(app_dir.join("game.exe"), "old").unwrap();
    std::fs::write(app_dir.join("save.sav"), "mine").unwrap();
    std::fs::write(incoming.join("game.exe"), "new").unwrap();
    std::fs::write(incoming.join("new.bin"), "new").unwrap();
    Journal {
        id: "x".into(),
        revision: "r2".into(),
        app_dir,
        incoming,
        backup_dir: root.join("backup"),
        to_backup: vec!["game.exe".into()],
        to_place: vec!["game.exe".into(), "new.bin".into()],
    }
}

#[test]
fn failed_apply_rolls_everything_back() {
    let dir = tempfile::tempdir().unwrap();
    let mut journal = journal_fixture(dir.path());
    journal.to_place.push("missing.bin".into());

    let err = apply(&journal, &CancelFlag::new(), &no_progress).unwrap_err();
    assert!(matches!(err, AppError::Io { .. }));
    assert_eq!(
        std::fs::read_to_string(journal.app_dir.join("game.exe")).unwrap(),
        "old"
    );
    assert!(!journal.app_dir.join("new.bin").exists());
    assert_eq!(
        std::fs::read_to_string(journal.incoming.join("game.exe")).unwrap(),
        "new"
    );
    assert_eq!(
        std::fs::read_to_string(journal.app_dir.join("save.sav")).unwrap(),
        "mine"
    );
}

#[test]
fn cancel_during_apply_rolls_back() {
    let dir = tempfile::tempdir().unwrap();
    let journal = journal_fixture(dir.path());
    let cancel = CancelFlag::new();
    let cancel_after_first = |p: Progress| {
        if matches!(p, Progress::Applying { done: 1, .. }) {
            cancel.cancel();
        }
    };
    let err = apply(&journal, &cancel, &cancel_after_first).unwrap_err();
    assert!(matches!(err, AppError::Cancelled));
    assert_eq!(
        std::fs::read_to_string(journal.app_dir.join("game.exe")).unwrap(),
        "old"
    );
}

fn write_db(dirs: &AppDirs, revision: &str) {
    let mut db = InstalledDb::default();
    db.apps.insert(
        "x".into(),
        InstalledApp {
            id: "x".into(),
            name: "X".into(),
            version: "v1".into(),
            asset_name: "a.zip".into(),
            asset_id: 1,
            asset_sha256: None,
            dir: PathBuf::from("app"),
            exe: "game.exe".into(),
            installed_at: Utc::now(),
            updated_at: None,
            files: BTreeMap::new(),
            previous: None,
            revision: revision.into(),
        },
    );
    db.save(&dirs.installed_file()).unwrap();
}

#[test]
fn recover_rolls_back_an_interrupted_swap() {
    let dir = tempfile::tempdir().unwrap();
    let dirs = AppDirs::at(dir.path().join("RecompHub"));
    dirs.ensure().unwrap();
    let lib = dirs.library(None);
    let journal = journal_fixture(dir.path());
    write_db(&dirs, "r1");
    fsutil::write_json(&journal_path(&lib, "x"), &journal).unwrap();
    std::fs::create_dir_all(lib.staging.join("x-20260101-000000")).unwrap();
    // Crash after the backup move and the first placement.
    fsutil::rename_retry(
        &journal.app_dir.join("game.exe"),
        &journal.backup_files().join("game.exe"),
    )
    .unwrap();
    fsutil::rename_retry(
        &journal.incoming.join("game.exe"),
        &journal.app_dir.join("game.exe"),
    )
    .unwrap();

    let report = recover(&dirs, std::slice::from_ref(&lib));
    assert_eq!(report.len(), 2, "{report:?}");
    assert_eq!(
        std::fs::read_to_string(journal.app_dir.join("game.exe")).unwrap(),
        "old"
    );
    assert_eq!(
        std::fs::read_to_string(journal.incoming.join("game.exe")).unwrap(),
        "new"
    );
    assert!(!journal.backup_dir.exists());
    assert!(!journal_path(&lib, "x").exists());
    assert!(!lib.staging.join("x-20260101-000000").exists());
}

#[test]
fn recover_keeps_a_committed_swap() {
    let dir = tempfile::tempdir().unwrap();
    let dirs = AppDirs::at(dir.path().join("RecompHub"));
    dirs.ensure().unwrap();
    let lib = dirs.library(None);
    let journal = journal_fixture(dir.path());
    apply(&journal, &CancelFlag::new(), &no_progress).unwrap();
    // installed.json already has the journal's revision: crash after commit.
    write_db(&dirs, &journal.revision);
    fsutil::write_json(&journal_path(&lib, "x"), &journal).unwrap();

    recover(&dirs, std::slice::from_ref(&lib));
    assert_eq!(
        std::fs::read_to_string(journal.app_dir.join("game.exe")).unwrap(),
        "new"
    );
    assert!(journal.backup_files().join("game.exe").exists());
    assert!(!journal_path(&lib, "x").exists());
}

#[test]
fn plan_respects_preserve_and_foreign_files() {
    let dir = tempfile::tempdir().unwrap();
    let app = dir.path();
    for rel in ["game.exe", "config/a.json", "user.txt"] {
        std::fs::create_dir_all(app.join(rel).parent().unwrap()).unwrap();
        std::fs::write(app.join(rel), "x").unwrap();
    }
    let old: BTreeMap<String, String> = ["game.exe", "config/a.json", "gone.bin"]
        .map(|r| (r.to_string(), String::new()))
        .into();
    let new: BTreeMap<String, String> = ["game.exe", "config/a.json", "user.txt", "fresh.bin"]
        .map(|r| (r.to_string(), String::new()))
        .into();
    let mut e = entry();
    e.preserve = vec!["config/*.json".into()];
    let (to_backup, to_place) = plan(app, &old, &new, &preserve_set(&e).unwrap());
    assert_eq!(to_backup, ["game.exe"]);
    assert_eq!(to_place, ["fresh.bin", "game.exe"]);
}
