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
3. [ ] Updates with `preserve`, backup of the previous version, automatic
   rollback with a transaction journal, "restore previous version",
   running-process check via sysinfo.
4. [ ] Game data modes (`copy_file`: hash check, v64/n64 byte swap to z64),
   library view, launch with log capture to `logs\<id>.log`.
5. [ ] Start menu / desktop shortcuts, Steam `shortcuts.vdf` (binary VDF,
   backup, no duplicates, Steam must be closed), gamepad navigation.
6. [ ] Custom sources ("Not verified"), self-update via tauri-plugin-updater,
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

## Open points

- Catalog asset patterns are unverified except Perfect Dark; the CI job
  `verify-catalog` checks them (see `catalog/RESEARCH.md`).
- The app is called RecompHub; the repository is ReCompX-Store. Rename not
  decided yet.
