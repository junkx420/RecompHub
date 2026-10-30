use recomphub_core::catalog::LoadedCatalog;
use recomphub_core::http::Http;
use recomphub_core::paths::AppDirs;
use recomphub_core::settings::Settings;
use recomphub_core::Result;
use tokio::sync::{Mutex, RwLock};

pub struct AppState {
    pub dirs: AppDirs,
    pub http: Http,
    pub settings: RwLock<Settings>,
    pub catalog: RwLock<Option<LoadedCatalog>>,
    /// Serializes catalog loads so parallel callers do not fetch twice.
    pub catalog_load: Mutex<()>,
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
        })
    }
}
