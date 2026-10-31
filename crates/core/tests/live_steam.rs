//! Reads this machine's Steam files without changing them.
//!
//! ```text
//! cargo test -p recomphub-core --test live_steam -- --ignored --nocapture
//! ```

use recomphub_core::platform;
use recomphub_core::vdf;

#[test]
#[ignore = "reads the local Steam installation"]
fn real_steam_files_parse_and_roundtrip() {
    let Some(steam) = platform::current().steam_dir() else {
        println!("Steam not installed; nothing to check");
        return;
    };
    println!("Steam at {}", steam.display());

    let login = std::fs::read_to_string(steam.join("config").join("loginusers.vdf")).unwrap();
    let doc = vdf::parse_text(&login).unwrap();
    let users = doc.get("users").expect("users key");
    println!("{} login user(s)", users.entries().len());

    let mut checked = 0;
    for entry in std::fs::read_dir(steam.join("userdata")).unwrap().flatten() {
        let path = entry.path().join("config").join("shortcuts.vdf");
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let parsed = vdf::parse(&bytes).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let count = parsed.map("shortcuts").map_or(0, |m| m.0.len());
        assert_eq!(
            vdf::write(&parsed),
            bytes,
            "{} does not roundtrip",
            path.display()
        );
        println!("{}: {count} shortcut(s), roundtrip exact", path.display());
        checked += 1;
    }
    println!("{checked} shortcuts.vdf file(s) checked");
}
