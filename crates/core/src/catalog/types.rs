use globset::Glob;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::LazyLock;

#[derive(Debug, Deserialize)]
pub(crate) struct CatalogDocument {
    pub schema_version: u32,
    /// Kept raw so that one malformed entry does not break the whole catalog.
    pub entries: Vec<serde_json::Value>,
}

/// Plain string or a map of language code to text (`"en"` is required).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum LocalizedText {
    Plain(String),
    Localized(BTreeMap<String, String>),
}

impl LocalizedText {
    pub fn get(&self, lang: &str) -> &str {
        match self {
            LocalizedText::Plain(s) => s,
            LocalizedText::Localized(map) => map
                .get(lang)
                .or_else(|| map.get("en"))
                .map_or("", String::as_str),
        }
    }

    fn validate(&self, field: &str, problems: &mut Vec<String>) {
        match self {
            LocalizedText::Plain(s) if s.trim().is_empty() => {
                problems.push(format!("{field} is empty"))
            }
            LocalizedText::Localized(map) if !map.contains_key("en") => {
                problems.push(format!("{field} needs an \"en\" text"))
            }
            _ => {}
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Game,
    Emulator,
    Tool,
}

/// How far along a port is. The UI lists experimental ones separately.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Maturity {
    /// Playable from start to finish, or close to it.
    #[default]
    Playable,
    /// Early: may crash, miss content or need workarounds.
    Experimental,
}

fn is_false(b: &bool) -> bool {
    !*b
}

/// Where an entry came from. Not part of the JSON; set by the loader.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EntrySource {
    #[default]
    Official,
    /// User-added source; shown as "not verified".
    Custom { url: String },
}

/// A ROM dump identified by hash. At least one of `sha1`/`md5` is set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnownDump {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha1: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub md5: Option<String>,
    pub region: String,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum GameData {
    /// The port needs no game data.
    None,
    /// The port asks for the user's data itself; we only show a hint.
    PortHandlesIt { hint: LocalizedText },
    /// We verify the user's file and copy it to `target` in the game folder.
    CopyFile {
        target: String,
        accepted_hashes: Vec<KnownDump>,
        /// Dumps we recognize but reject, for precise error messages.
        #[serde(default)]
        other_known_hashes: Vec<KnownDump>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        hint: Option<LocalizedText>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogEntry {
    pub id: String,
    pub name: String,
    pub category: Category,
    pub system: String,
    /// `owner/name` on GitHub.
    pub repo: String,
    /// Regex on the release asset name; must match exactly one asset.
    pub asset_pattern: String,
    /// Fixed release tag instead of "latest" (for rolling CI releases).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release_tag: Option<String>,
    /// Use the newest release even if it is a pre-release, for projects
    /// that have not published a stable release yet.
    #[serde(default, skip_serializing_if = "is_false")]
    pub include_prereleases: bool,
    #[serde(default)]
    pub maturity: Maturity,
    /// Executable, relative to the game folder; glob allowed.
    pub exe: String,
    /// Globs of files that updates never overwrite.
    #[serde(default)]
    pub preserve: Vec<String>,
    /// Data the port keeps outside its folder (saves/config), e.g.
    /// `%LOCALAPPDATA%\Zelda64Recompiled`. Informational; only touched when
    /// the user explicitly removes saves on uninstall.
    #[serde(default)]
    pub user_data_paths: Vec<String>,
    pub game_data: GameData,
    pub description: LocalizedText,
    #[serde(default)]
    pub credits: Vec<String>,
    pub license: String,
    pub homepage: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_url: Option<String>,
    #[serde(default, skip_deserializing)]
    pub source: EntrySource,
}

static ID_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[a-z0-9][a-z0-9-]{0,63}$").unwrap());
static REPO_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$").unwrap());
static TAG_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[A-Za-z0-9._+-]+$").unwrap());
static SYSTEM_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[a-z0-9]{1,16}$").unwrap());
const USER_DATA_ROOTS: &[&str] = &["%LOCALAPPDATA%\\", "%APPDATA%\\", "%USERPROFILE%\\"];

/// Relative path that cannot escape its base: no root, drive or `..`.
pub fn is_safe_relative(path: &str) -> bool {
    if path.is_empty() || path.starts_with(['/', '\\']) || path.contains(':') {
        return false;
    }
    !path.split(['/', '\\']).any(|part| part == "..")
}

fn is_https_url(s: &str) -> bool {
    url::Url::parse(s).is_ok_and(|u| u.scheme() == "https")
}

fn is_hex(s: &str, len: usize) -> bool {
    s.len() == len && s.bytes().all(|b| b.is_ascii_hexdigit())
}

impl KnownDump {
    fn validate(&self, field: &str, problems: &mut Vec<String>) {
        if self.sha1.is_none() && self.md5.is_none() {
            problems.push(format!("{field}: needs sha1 or md5"));
        }
        if self.sha1.as_deref().is_some_and(|h| !is_hex(h, 40)) {
            problems.push(format!("{field}: sha1 must be 40 hex digits"));
        }
        if self.md5.as_deref().is_some_and(|h| !is_hex(h, 32)) {
            problems.push(format!("{field}: md5 must be 32 hex digits"));
        }
    }
}

impl CatalogEntry {
    /// All problems with this entry; empty means valid.
    pub fn validate(&self) -> Vec<String> {
        let mut p = Vec::new();
        if !ID_RE.is_match(&self.id) {
            p.push("id must match ^[a-z0-9][a-z0-9-]{0,63}$".into());
        }
        if self.name.trim().is_empty() {
            p.push("name is empty".into());
        }
        if !SYSTEM_RE.is_match(&self.system) {
            p.push("system must be a short lowercase code".into());
        }
        if !REPO_RE.is_match(&self.repo) {
            p.push("repo must be owner/name".into());
        }
        if let Err(e) = Regex::new(&self.asset_pattern) {
            p.push(format!("asset_pattern: {e}"));
        }
        if self
            .release_tag
            .as_deref()
            .is_some_and(|t| !TAG_RE.is_match(t))
        {
            p.push("release_tag must match ^[A-Za-z0-9._+-]+$".into());
        }
        for (field, glob) in
            std::iter::once(("exe", &self.exe)).chain(self.preserve.iter().map(|g| ("preserve", g)))
        {
            if !is_safe_relative(glob) {
                p.push(format!(
                    "{field} {glob:?} must be a relative path inside the game folder"
                ));
            } else if let Err(e) = Glob::new(glob) {
                p.push(format!("{field} {glob:?}: {e}"));
            }
        }
        for path in &self.user_data_paths {
            let rooted = USER_DATA_ROOTS.iter().any(|r| path.starts_with(r));
            let rest_ok = path.split('\\').skip(1).all(|s| !s.is_empty() && s != "..");
            if !rooted || !rest_ok {
                p.push(format!("user_data_paths {path:?} must start with %LOCALAPPDATA%\\, %APPDATA%\\ or %USERPROFILE%\\"));
            }
        }
        match &self.game_data {
            GameData::None => {}
            GameData::PortHandlesIt { hint } => hint.validate("game_data.hint", &mut p),
            GameData::CopyFile {
                target,
                accepted_hashes,
                other_known_hashes,
                hint,
            } => {
                if !is_safe_relative(target) {
                    p.push("game_data.target must be a relative path".into());
                }
                if accepted_hashes.is_empty() {
                    p.push("game_data.accepted_hashes is empty".into());
                }
                for d in accepted_hashes {
                    d.validate("game_data.accepted_hashes", &mut p);
                }
                for d in other_known_hashes {
                    d.validate("game_data.other_known_hashes", &mut p);
                }
                if let Some(h) = hint {
                    h.validate("game_data.hint", &mut p);
                }
            }
        }
        self.description.validate("description", &mut p);
        if self.license.trim().is_empty() {
            p.push("license is empty".into());
        }
        if !is_https_url(&self.homepage) {
            p.push("homepage must be an https URL".into());
        }
        if self.icon_url.as_deref().is_some_and(|u| !is_https_url(u)) {
            p.push("icon_url must be an https URL".into());
        }
        p
    }

    /// `https://api.github.com/repos/<repo>/releases/...` for this entry. With
    /// `include_prereleases` it is the release list; the newest entry wins.
    pub fn release_api_url(&self) -> String {
        match &self.release_tag {
            Some(tag) => format!(
                "https://api.github.com/repos/{}/releases/tags/{tag}",
                self.repo
            ),
            None if self.include_prereleases => format!(
                "https://api.github.com/repos/{}/releases?per_page=10",
                self.repo
            ),
            None => format!("https://api.github.com/repos/{}/releases/latest", self.repo),
        }
    }
}
