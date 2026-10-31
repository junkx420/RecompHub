# RecompHub

A store for native PC ports of recompiled and decompiled games
(N64Recomp, XenonRecomp, decomp projects) on Windows 10/11 x64. It finds,
installs, updates and launches the ports from their official GitHub
releases, keeps your saves safe across updates, and puts the games in your
start menu and Steam library.

> **Status:** all six planned milestones are implemented; the app has not
> had a public release yet. Uninstalling a port from inside the app is not
> implemented yet.

**Legal:** RecompHub never downloads, hosts or links ROMs, ISOs or other
game data. It only downloads the release assets that port projects publish
on GitHub. Ports need your own legally obtained copy of each game.

## Features

- **Catalog** of curated ports (`catalog/catalog.json`), loaded from this
  repository with an offline fallback. Custom catalogs can be added in
  Settings; their entries are marked "Not verified".
- **Install** with download resume, SHA-256 check against the digest GitHub
  publishes for each asset, zip and 7z support, and protection against
  archives that try to write outside their folder. Folders that RecompHub
  did not create are never overwritten without asking.
- **Updates that keep your data.** Only the files the old release installed
  are replaced; saves, settings, mods and anything the port or you created
  stay where they are. The previous version is backed up and can be
  restored with one click. An update that fails, or is interrupted by a
  crash, is rolled back automatically.
- **Game data.** For ports that need a ROM file in their folder, RecompHub
  checks your dump against the known hashes, converts `.v64`/`.n64` to
  `.z64`, and tells you exactly which dump you have if it is the wrong one.
- **Launch** from the app, with each game's console output in
  `%LOCALAPPDATA%\RecompHub\logs\<id>.log`.
- **Start menu, desktop and Steam.** Shortcuts are created on install;
  games can be added to Steam as non-Steam shortcuts for any account on the
  PC (Steam must be closed; `shortcuts.vdf` is backed up first).
- **Controller support:** D-pad or left stick moves, A selects, B goes back,
  LB/RB switch tabs.
- **Self-update** of RecompHub itself from signed GitHub releases.
- English and German interface.

## Install

Download `RecompHub_<version>_x64-setup.exe` from the
[latest release](https://github.com/junkx420/ReCompX-Store/releases/latest)
and run it. It installs for the current user; no administrator rights are
needed. The WebView2 runtime is installed if it is missing.

Data lives in `%LOCALAPPDATA%\RecompHub` (settings, `installed.json`,
logs) and games in `%LOCALAPPDATA%\RecompHub\apps` unless you pick another
library folder in Settings.

## Adding a port to the catalog

Edit `catalog/catalog.json` (schema: `catalog/catalog.schema.json`). Each
entry names the GitHub repository, a regular expression that matches
exactly one Windows asset of its releases, the executable, the files that
updates must keep (`preserve`), and how the port gets its game data.
`catalog/RESEARCH.md` records how the existing entries were checked. CI
verifies every pattern against the live GitHub releases on each push and
weekly.

A custom catalog uses the same format; host it anywhere over https and add
its URL in Settings, Custom sources.

## Development

Requirements: Rust (stable), Node.js 22, and the Visual Studio C++ build
tools on Windows.

```bash
npm ci
npm run tauri dev           # run the app
npm run dev                 # UI only, in a browser, with a mock backend
cargo test --workspace      # Rust tests
npm test                    # frontend tests
npm run lint && npm run typecheck
```

Tests that touch the network or the local Steam installation are ignored
by default:

```bash
cargo test -p recomphub-core --test live_install -- --ignored --nocapture
cargo test -p recomphub-core --test live_steam -- --ignored --nocapture
```

Layout: `crates/core` holds all logic and has no Tauri dependency,
`src-tauri` is a thin command layer, `src` is the React UI, `catalog` the
catalog and its schema. Design decisions and the full command list are in
[`docs/PLAN.md`](docs/PLAN.md).

## Releasing

Self-update needs a signing key pair. Once:

1. Run `npx tauri signer generate -w recomphub.key` and keep the private
   key and its password safe (never commit them).
2. Add the repository secrets `TAURI_SIGNING_PRIVATE_KEY` (the contents of
   `recomphub.key`) and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.
3. Put the public key (`recomphub.key.pub`) into
   `plugins.updater.pubkey` in `src-tauri/tauri.conf.json`.

For each release, set the same version in `package.json`, `Cargo.toml`
(`[workspace.package]`) and `src-tauri/tauri.conf.json`, commit, and push a
tag `v<version>`. The release workflow checks the versions, runs the
tests, builds the signed installer and creates a draft release with
`latest.json`. Publish the draft to make it visible to the updater.

Builds without a public key (local builds, forks) simply have self-update
switched off.

## License

GPL-3.0-or-later. The ports themselves have their own licenses, shown on
each catalog entry.
