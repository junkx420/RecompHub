# RecompHub – plan and status

Handover notes so any session (cloud or local) can continue the work.

## Agreed decisions

- Stack: Tauri 2, Rust backend (`crates/core` holds all logic, `src-tauri` is a
  thin command layer), React + TypeScript + Vite frontend.
- Target: Windows 10/11 x64 only. OS code lives in
  `crates/core/src/platform/windows.rs` behind the `Platform` trait.
- Never download, host or link ROMs, ISOs or game data. Only official GitHub
  release assets of the port projects.
- Updates never lose user data: `preserve` globs win, files not in the
  install manifest (created by the port or user) are always carried over.
- Never overwrite foreign content: install preflight reports
  `ForeignContent` and the UI asks first.
- Staging and backups live on the same volume as the library:
  default library uses `%LOCALAPPDATA%\RecompHub\{staging,backups}`, a custom
  library uses `<library>\.recomphub\{staging,backups}` (atomic rename).
- GitHub token is stored in the Windows Credential Manager, never in
  settings.json, and only sent to api.github.com.
- Updater signing key: the owner generates it with `npx tauri signer generate`
  and stores the private key as GitHub secret `TAURI_SIGNING_PRIVATE_KEY`.
  Only the public key goes into `tauri.conf.json`.
- Default catalog URL:
  `https://raw.githubusercontent.com/junkx420/ReCompX-Store/main/catalog/catalog.json`.
- Catalog extensions beyond the original spec: `release_tag` (rolling CI
  releases), `user_data_paths` (saves outside the game folder), `md5` as an
  alternative to `sha1` in known dumps, `other_known_hashes` for precise
  wrong-dump messages.
- Downloads run sequentially. Locked files: 5 retries, 100 ms to 1.6 s backoff.
- Steam with several accounts: picker, preselect the most recent login.
- Archives: zip and 7z (`sevenz-rust2`, maintained fork of `sevenz-rust`).
- Repository language is English only (code, comments, docs, commits). The
  app UI stays bilingual: `src/i18n/locales/de.json` and the `de` strings in
  the catalog are translations, not repository prose.

## Planned interface

Commands: `settings_get/set`, `catalog_get`, `catalog_refresh`,
`source_add/remove/list`, `release_info(id)`, `install_preflight(id)`,
`install(id, opts)`, `update(id)`, `job_cancel(job)`, `restore_previous(id)`,
`uninstall(id, keep_saves)`, `library_list`, `updates_check`,
`gamedata_status(id)`, `gamedata_import(id, path)`, `launch(id)`,
`open_folder(id)`, `open_log(id)`, `shortcut_create(id, desktop)`,
`shortcut_remove(id)`, `steam_status`, `steam_add(id, user_ids)`.

Events: `job:progress`, `job:finished`, `job:failed`, `updates:available`,
`game:exited`.

Errors: `AppError` in `crates/core/src/error.rs`, serialized as
`{ code, params }` and translated in the frontend (`errors.<code>`).

## Milestones

1. [x] Scaffold, settings, i18n, catalog loading and display.
2. [x] GitHub releases, download (progress, cancel, Range resume, SHA-256
   against asset `digest`), unpack to staging (zip-slip safe, flatten single
   root), install a real game end to end, `installed.json` with file hashes.
   Live check: `cargo test -p recomphub-core --test live_install -- --ignored`
   (set `RECOMPHUB_LIVE_ID` for another entry).
3. [x] Updates with `preserve`, backup of the previous version, automatic
   rollback with a transaction journal, "restore previous version",
   running-process check via sysinfo. Live check:
   `cargo test -p recomphub-core --test live_install updates_real -- --ignored`.
4. [x] Game data modes (`copy_file`: hash check, v64/n64 byte swap to z64),
   library view, launch with log capture to `logs\<id>.log`.
5. [x] Start menu / desktop shortcuts, Steam `shortcuts.vdf` (binary VDF,
   backup, no duplicates, Steam must be closed), gamepad navigation.
6. [x] Custom sources ("Not verified"), self-update via tauri-plugin-updater,
   release workflow on `v*` tags, polished README.

Done when a fresh Windows user runs the installer, installs Zelda64Recomp
with one click, launches it, updates without losing saves, and finds it in
Steam.

## Implemented interface (milestone 2)

`release_info`, `install_preflight`, `install` (returns a job id; outcome via
`job:progress` / `job:finished` / `job:failed`), `job_cancel`, `library_list`.
Jobs run one at a time (`AppState::job_slot`). A cancelled download keeps its
`.part` file and resumes on the next attempt. Unpacking needs the
multi-threaded Tokio runtime (`block_in_place`).

## Implemented interface (milestone 3)

`update`, `restore_previous` (jobs, same events as `install`; progress phase
`applying`), `updates_check` (list of `{ id, installed, latest }`). The UI
checks once per start when `check_updates` is on.

Update design (`crates/core/src/update.rs`): files are swapped in place.
Release-owned files (`InstalledApp::files`) not matching `preserve` move to
`<backups>/<id>/<version>-<revision>/files`; new release files move in where
the path is free or was freed. Everything else in the game folder is never
touched, even if the new release ships a file of the same name. The plan
is written to `<staging>/<id>.journal.json` first; failures roll back at
once, crashes on the next start (`update::recover`, called before the UI
starts). A journal counts as committed when `installed.json` already has
its `revision`. One backup per app is kept; a restore makes the current
version the new backup.

## Implemented interface (milestone 4)

`gamedata_status`, `gamedata_import(id, path)` (`crates/core/src/gamedata.rs`:
detects z64/v64/n64 by the first four bytes, converts to z64, matches
SHA-1/MD5 against `accepted_hashes`, names the dump if it is one of
`other_known_hashes`, writes the target atomically), `launch` (working
directory is the game folder, stdout/stderr to `logs\<id>.log`, no extra
console window, `game:exited { id, code }` when it ends), `running_list`,
`open_log`, `log_tail`. Launch refuses when a `copy_file` dump is missing
or a program from the game folder is already running. The imported dump is
not in the release manifest, so updates keep it.

## Implemented interface (milestone 5)

`shortcut_create(id, desktop)`, `shortcut_remove`, `steam_status`
(`{ found, running, users }`, most recent login first), `steam_add(id,
user_ids)`, `steam_remove`. A fresh install gets a start menu link in
`Programs\RecompHub` (plus a desktop link if `desktop_shortcut` is on);
updates and restores rewrite existing links and Steam entries, because the
executable may have changed. Links and Steam accounts are recorded in
`InstalledApp::shortcuts` / `steam_users`.

Steam (`crates/core/src/steam.rs`, `vdf.rs`): entries are matched by
`StartDir` (the game folder), so re-adding updates instead of duplicating
and keeps the appid. The binary VDF reader rejects anything it cannot write
back byte for byte. Each write backs up `shortcuts.vdf` first (newest 3
kept). `steam_add`/`steam_remove` refuse while `steam.exe` runs.

Gamepad (`src/gamepad.ts`): polled every 33 ms (animation frames stall in
throttled windows); D-pad/left stick move focus spatially, A clicks, B
clicks the element marked `data-gamepad-back`, LB/RB switch tabs.

## Implemented interface (milestone 6)

`source_add(url)` (fetches and validates the catalog before saving),
`source_remove(url)`; `settings.custom_sources` feeds
`catalog::load_with_sources`, where official ids win over custom ones.
Installing a custom entry asks for confirmation.

Self-update: `tauri-plugin-updater` is registered only when
`plugins.updater.pubkey` in `tauri.conf.json` is set (`app_info.self_update`
tells the UI). The endpoint is `releases/latest/download/latest.json`.
`.github/workflows/release.yml` runs on `v*` tags: checks that the tag
matches all three version fields and that the signing secret and pubkey
exist, tests, then `tauri-action` builds with
`src-tauri/tauri.release.conf.json` (`createUpdaterArtifacts`) and creates a
draft release with `latest.json`.

## Catalog maturity

Entries carry `maturity` (`playable` by default, or `experimental`) and
optionally `include_prereleases` (use the newest release even if it is a
pre-release; `github::fetch_release` then reads the release list). The UI
shows playable entries under Discover and experimental ones in their own
tab. How the 31 October 2026 expansion was selected is in
`catalog/RESEARCH.md`. A download the virus scanner blocks (Windows error
225/226) is reported as `blocked_by_antivirus`.

## Docs and maintenance

`README.md` is for players. Developer setup is in `docs/DEVELOPMENT.md`,
releases, signing and the catalog refresh in `docs/RELEASING.md`, catalog
contributions in `CONTRIBUTING.md`. The weekly
`.github/workflows/catalog-refresh.yml` runs `tools/catalog-scout/scout.py`
and opens a pull request; it needs "Allow GitHub Actions to create and
approve pull requests" enabled in the repository settings.

## Uninstall

`uninstall(id, keep_saves)` (`crates/core/src/uninstall.rs`): refuses while
the game runs or a job works on it, and while Steam runs if the app has
Steam entries. The game folder is renamed into staging first, so a locked
file fails the uninstall cleanly. With `keep_saves`, every file the release
did not place, plus `preserve` matches, moves to
`<backups>\<id>\saves-<time>`; the next install of the same id moves those
files back where the new release has no file of the same path. Without
`keep_saves`, the entry's `user_data_paths` are deleted too. Then the
record, the previous-version backup, shortcuts and Steam entries go.

## Open points

- Updater key: public key (minisign id CD3676152454D674) is in
  `tauri.conf.json`; the private key and password are repository secrets
  and with the owner. Losing them means installed copies can no longer be
  updated.


- Testing from a Claude session: Claude desktop is an MSIX app, so a
  RecompHub started from its shells writes `%LOCALAPPDATA%` (and the start
  menu under `%APPDATA%`) into the package's virtualized folder, which
  Explorer cannot open. Start the app yourself for realistic paths.

- Catalog asset patterns are unverified except Perfect Dark; the CI job
  `verify-catalog` checks them (see `catalog/RESEARCH.md`).
- The app is called RecompHub; the repository is ReCompX-Store. Rename not
  decided yet.
