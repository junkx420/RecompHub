# RecompHub

RecompHub finds, installs, updates and launches native PC ports of
recompiled and decompiled games (N64Recomp, XenonRecomp, decomp projects)
on Windows 10/11 x64.

> **Status:** early development (milestone 4 of 6). Ports can be installed,
> updated without losing saves, rolled back and launched; your own ROM dump
> is checked and imported where a port needs it. Shortcuts, Steam
> integration and self-update are not implemented yet.

**Legal:** RecompHub never downloads, hosts or links ROMs, ISOs or other
game data. It only downloads the official release assets that port projects
publish on GitHub. You need your own legally obtained copy of each game.

## Development

Requirements: Rust (stable), Node.js 22, and on Windows the WebView2 runtime.

```bash
npm ci
npm run tauri dev      # run the app
cargo test --workspace # Rust tests
npm test               # frontend tests
```

`npm run dev` alone starts the UI in a browser with a mock backend.

Layout: `crates/core` (all logic, no Tauri), `src-tauri` (commands),
`src` (React UI), `catalog` (catalog and JSON schema).

License: GPL-3.0-or-later.
