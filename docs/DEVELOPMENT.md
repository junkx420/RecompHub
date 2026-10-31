# Development

Requirements: Rust (stable), Node.js 22, and on Windows the Visual Studio C++ build
tools.

```bash
npm ci
npm run tauri dev           # run the app
npm run dev                 # UI only, in a browser, with a mock backend
cargo test --workspace      # Rust tests
npm test                    # frontend tests
npm run lint && npm run typecheck
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings
```

Tests that touch the network or the local Steam installation are ignored by default:

```bash
cargo test -p recomphub-core --test live_install -- --ignored --nocapture
cargo test -p recomphub-core --test live_steam -- --ignored --nocapture
```

## Layout

- `crates/core`: all logic (catalog, downloads, install, updates, uninstall, game
  data, launch, shortcuts, Steam). No Tauri dependency, so it is tested against
  temporary folders and a mock HTTP server.
- `src-tauri`: thin Tauri command layer and job runner.
- `src`: React UI. `src/api/mock.ts` is the browser mock backend.
- `catalog`: the catalog, its schema and research notes.
- `tools/catalog-scout`: finds and refreshes catalog entries.

Design decisions and the full command list are in [PLAN.md](PLAN.md).

## Notes

- App data lives in `%LOCALAPPDATA%\RecompHub`. When the app is started from an MSIX
  packaged host (for example a sandboxed terminal), Windows redirects that folder into
  the host's package folder; start it from a normal terminal for realistic paths.
- CI (`.github/workflows/ci.yml`) runs lint, type checks, tests, clippy and builds the
  installer on every push; it also checks every catalog pattern against GitHub.
