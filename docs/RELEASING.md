# Releasing and maintenance

For the maintainer of this repository.

## Signing key (once)

Self-update only accepts releases signed with the project key.

1. `npx tauri signer generate -w recomphub.key` and keep the private key and its
   password safe, outside the repository. Losing them means installed copies can no
   longer be updated.
2. Repository secrets `TAURI_SIGNING_PRIVATE_KEY` (contents of `recomphub.key`) and
   `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.
3. The public key (`recomphub.key.pub`) goes into `plugins.updater.pubkey` in
   `src-tauri/tauri.conf.json`.

Builds without a public key (local builds, forks) have self-update switched off.

## A new version

1. Set the same version in `package.json`, `Cargo.toml` (`[workspace.package]`) and
   `src-tauri/tauri.conf.json`, and commit.
2. Push a tag `v<version>`.
3. `.github/workflows/release.yml` checks the versions and signing setup, runs the
   tests, builds the signed installer and creates a **draft** release with
   `latest.json`.
4. Review the draft on GitHub and publish it. Installed copies see it from then on.

If the build fails, fix the cause and re-run the failed job; the tag can stay.

## Catalog refresh

`tools/catalog-scout/scout.py` keeps the catalog current from the rated
[awesome-decomp-games](https://github.com/SubmitGame/awesome-decomp-games) data:

- adds new recompilations and console decomp ports that have a Windows release
  (at least 50 stars, active, not flagged as AI-generated);
- for entries it manages (`managed` in `catalog/scout-state.json`): rebuilds broken
  asset patterns, drops `include_prereleases` once a stable release exists, and
  promotes experimental ports that reach a stable 1.0 release;
- never touches hand-written entries and never adds repositories listed under
  `excluded`.

`.github/workflows/catalog-refresh.yml` runs it every Monday (and on demand), installs
every new or changed entry for real on a Windows runner, drops entries whose install
fails, and opens a pull request with a summary. Review and merge it; the app picks up
the catalog from `main` on its next start.

Run it locally with `GITHUB_TOKEN` set:

```bash
python -I tools/catalog-scout/scout.py --summary summary.md --changed changed.txt
```

Selection rules and past exclusions are documented in
[`catalog/RESEARCH.md`](../catalog/RESEARCH.md).
