use recomphub_core::catalog::LoadedCatalog;
use recomphub_core::http::Http;
use recomphub_core::job::CancelFlag;
use recomphub_core::paths::AppDirs;
use recomphub_core::settings::Settings;
use recomphub_core::Result;
use std::collections::HashMap;
use std::sync::atomic::AtomicU64;
use tokio::sync::{Mutex, RwLock};

pub struct RunningJob {
    pub app_id: String,
    pub cancel: CancelFlag,
}

pub struct AppState {
    pub dirs: AppDirs,
    pub http: Http,
    pub settings: RwLock<Settings>,
    pub catalog: RwLock<Option<LoadedCatalog>>,
    /// Serializes catalog loads so parallel callers do not fetch twice.
    pub catalog_load: Mutex<()>,
    /// Queued and running jobs by id.
    pub jobs: std::sync::Mutex<HashMap<u64, RunningJob>>,
    pub next_job: AtomicU64,
    /// Jobs run one at a time: downloads are sequential and every job
    /// rewrites `installed.json`.
    pub job_slot: Mutex<()>,
}

impl AppState {
    pub fn new(dirs: AppDirs) -> Result<Self> {
        let settings = Settings::load(&dirs.settings_file());
        let http = Http::new(dirs.cache())?;
        Ok(Self {
            dirs,
            http,
            settings: RwLock::new(settings),
            catalog: RwLock::new(None),
            catalog_load: Mutex::new(()),
            jobs: std::sync::Mutex::new(HashMap::new()),
            next_job: AtomicU64::new(1),
            job_slot: Mutex::new(()),
        })
    }
}
