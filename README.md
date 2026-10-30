# RecompHub

**English** · [Deutsch](#deutsch)

RecompHub finds, installs, updates and launches native PC ports of
recompiled and decompiled games (N64Recomp, XenonRecomp, decomp projects)
on Windows 10/11 x64.

> **Status:** early development (milestone 1 of 6: scaffold, settings,
> i18n, catalog). Installing ports is not implemented yet.

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

---

## Deutsch

RecompHub findet, installiert, aktualisiert und startet native PC-Ports
recompilierter und dekompilierter Spiele (N64Recomp, XenonRecomp,
Decomp-Projekte) unter Windows 10/11 x64.

> **Stand:** frühe Entwicklung (Meilenstein 1 von 6: Gerüst, Einstellungen,
> i18n, Katalog). Installieren ist noch nicht umgesetzt.

**Rechtliches:** RecompHub lädt, hostet und verlinkt niemals ROMs, ISOs
oder andere Spieldaten. Die App lädt nur die offiziellen Release-Assets,
die Port-Projekte auf GitHub veröffentlichen. Du brauchst eine eigene,
legal erworbene Kopie jedes Spiels.

Entwicklung: siehe oben. Lizenz: GPL-3.0-or-later.
