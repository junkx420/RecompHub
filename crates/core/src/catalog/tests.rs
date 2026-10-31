use super::*;
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

fn entry_json(id: &str) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "name": "Test Port",
        "category": "game",
        "system": "n64",
        "repo": "owner/repo",
        "asset_pattern": "(?i)windows.*\\.zip$",
        "exe": "Game.exe",
        "preserve": ["saves/**", "*.ini"],
        "game_data": {"mode": "none"},
        "description": {"en": "A port", "de": "Ein Port"},
        "license": "GPL-3.0",
        "homepage": "https://github.com/owner/repo"
    })
}

fn doc(entries: Vec<serde_json::Value>) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({"schema_version": 1, "entries": entries})).unwrap()
}

#[test]
fn embedded_catalog_is_valid_without_warnings() {
    let (entries, warnings) = parse(EMBEDDED_CATALOG.as_bytes(), EntrySource::Official).unwrap();
    assert!(warnings.is_empty(), "{warnings:#?}");
    assert!(entries.len() >= 8);
    assert!(entries.iter().any(|e| e.id == "zelda64recomp"));
}

#[test]
fn embedded_catalog_matches_json_schema() {
    let schema: serde_json::Value =
        serde_json::from_str(include_str!("../../../../catalog/catalog.schema.json")).unwrap();
    let instance: serde_json::Value = serde_json::from_str(EMBEDDED_CATALOG).unwrap();
    let validator = jsonschema::validator_for(&schema).unwrap();
    let errors: Vec<String> = validator
        .iter_errors(&instance)
        .map(|e| format!("{e} at {}", e.instance_path()))
        .collect();
    assert!(errors.is_empty(), "{errors:#?}");
}

#[test]
fn invalid_entries_are_dropped_with_warning() {
    let mut bad = entry_json("bad");
    bad["exe"] = "../outside.exe".into();
    let mut no_name = entry_json("noname");
    no_name.as_object_mut().unwrap().remove("name");
    let json = doc(vec![entry_json("good"), bad, no_name, entry_json("good")]);

    let (entries, warnings) = parse(&json, EntrySource::Official).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].id, "good");
    assert_eq!(warnings.len(), 3, "{warnings:#?}");
    assert!(warnings.iter().any(|w| w.contains("duplicate")));
}

#[test]
fn newer_schema_version_is_rejected() {
    let json =
        serde_json::to_vec(&serde_json::json!({"schema_version": 99, "entries": []})).unwrap();
    assert!(matches!(
        parse(&json, EntrySource::Official),
        Err(AppError::CatalogInvalid { .. })
    ));
}

#[test]
fn custom_source_is_tagged() {
    let source = EntrySource::Custom {
        url: "https://example.com/c.json".into(),
    };
    let (entries, _) = parse(&doc(vec![entry_json("x")]), source.clone()).unwrap();
    assert_eq!(entries[0].source, source);
}

#[test]
fn safe_relative_paths() {
    assert!(types::is_safe_relative("saves/**"));
    assert!(types::is_safe_relative("data\\pd.z64"));
    for bad in ["", "/abs", "\\abs", "C:\\x", "a/../../b", "..\\x"] {
        assert!(!types::is_safe_relative(bad), "{bad}");
    }
}

#[test]
fn select_asset_requires_exactly_one_match() {
    let names = [
        "Zelda64Recompiled-v1.2.2-Windows.zip",
        "Zelda64Recompiled-v1.2.2-Linux-X64.AppImage.zip",
        "Zelda64Recompiled-v1.2.2-macOS.zip",
    ];
    assert_eq!(
        select_asset(r"(?i)-Windows\.zip$", &names).unwrap(),
        "Zelda64Recompiled-v1.2.2-Windows.zip"
    );
    assert!(matches!(
        select_asset(r"\.zip$", &names),
        Err(AppError::AssetAmbiguous { matches, .. }) if matches.len() == 3
    ));
    assert!(matches!(
        select_asset(r"\.7z$", &names),
        Err(AppError::AssetNoMatch { candidates, .. }) if candidates.len() == 3
    ));
    assert!(select_asset("(", &names).is_err());
}

#[test]
fn localized_text_falls_back_to_english() {
    let text: LocalizedText = serde_json::from_str(r#"{"en":"Hello","de":"Hallo"}"#).unwrap();
    assert_eq!(text.get("de"), "Hallo");
    assert_eq!(text.get("fr"), "Hello");
    let plain: LocalizedText = serde_json::from_str(r#""Hi""#).unwrap();
    assert_eq!(plain.get("de"), "Hi");
}

#[tokio::test]
async fn load_prefers_remote_then_falls_back_to_embedded() {
    let server = MockServer::start().await;
    let dir = tempfile::tempdir().unwrap();
    let http = Http::new(dir.path()).unwrap();
    let url = format!("{}/catalog.json", server.uri());

    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(200).set_body_bytes(doc(vec![entry_json("remote-only")])),
        )
        .up_to_n_times(1)
        .mount(&server)
        .await;
    let loaded = load(&http, &url).await;
    assert_eq!(loaded.origin, CatalogOrigin::Remote);
    assert_eq!(loaded.entries.len(), 1);

    // Server now returns garbage: embedded copy is used.
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_string("{oops"))
        .mount(&server)
        .await;
    let loaded = load(&http, &url).await;
    assert_eq!(loaded.origin, CatalogOrigin::Embedded);
    assert!(!loaded.warnings.is_empty());
    assert!(loaded.get("zelda64recomp").is_some());
}

#[tokio::test]
async fn load_uses_cache_when_offline() {
    let server = MockServer::builder().start().await; // not pooled: drop stops it
    let dir = tempfile::tempdir().unwrap();
    let http = Http::new(dir.path()).unwrap();
    let url = format!("{}/catalog.json", server.uri());
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("ETag", "\"abc\"")
                .set_body_bytes(doc(vec![entry_json("cached-one")])),
        )
        .mount(&server)
        .await;
    load(&http, &url).await;
    drop(server);

    let loaded = load(&http, &url).await;
    assert_eq!(loaded.origin, CatalogOrigin::Cache);
    assert!(loaded.get("cached-one").is_some());
}

#[tokio::test]
async fn custom_sources_are_merged_tagged_and_cannot_shadow_official_ids() {
    use wiremock::matchers::path;
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/official.json"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(doc(vec![entry_json("shared")])))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/custom.json"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_bytes(doc(vec![entry_json("shared"), entry_json("extra")])),
        )
        .mount(&server)
        .await;
    let dir = tempfile::tempdir().unwrap();
    let http = Http::new(dir.path()).unwrap();
    let custom = format!("{}/custom.json", server.uri());
    let broken = format!("{}/missing.json", server.uri());

    let loaded = load_with_sources(
        &http,
        &format!("{}/official.json", server.uri()),
        &[custom.clone(), broken.clone()],
    )
    .await;
    let ids: Vec<_> = loaded.entries.iter().map(|e| e.id.as_str()).collect();
    assert_eq!(ids, ["shared", "extra"]);
    assert_eq!(loaded.get("shared").unwrap().source, EntrySource::Official);
    assert_eq!(
        loaded.get("extra").unwrap().source,
        EntrySource::Custom {
            url: custom.clone()
        }
    );
    assert!(loaded.warnings.iter().any(|w| w.contains("shared skipped")));
    assert!(loaded.warnings.iter().any(|w| w.starts_with(&broken)));
}
