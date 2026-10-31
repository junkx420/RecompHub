# Contributing

Thanks for helping. There are three common ways to contribute.

## Report a problem

Open an [issue](https://github.com/junkx420/ReCompX-Store/issues) with what you did,
what happened, and the RecompHub version (Settings, About). Logs are in
`%LOCALAPPDATA%\RecompHub\logs`; each game's own output is in `logs\<id>.log`.

Problems inside a game belong to that port's project, not here.

## Add or fix a port

The catalog is [`catalog/catalog.json`](catalog/catalog.json), checked against
[`catalog/catalog.schema.json`](catalog/catalog.schema.json). An entry names the
GitHub repository, a regular expression that matches exactly one Windows asset of its
releases, the executable, files that updates must keep (`preserve`), and how the port
gets its game data. Set `"maturity": "experimental"` for ports that are still early.

Before you open a pull request:

```bash
cargo test -p recomphub-core catalog
GITHUB_TOKEN=... cargo run -p recomphub-core --bin verify-catalog
RECOMPHUB_LIVE_ID=<id> cargo test -p recomphub-core --test live_install -- --ignored --nocapture
```

The last command downloads and installs the port into a temporary folder.

Most entries are maintained by the catalog scout (see
[docs/RELEASING.md](docs/RELEASING.md#catalog-refresh)), which runs weekly. To keep a
repository out of the catalog for good, add it to `excluded` in
[`catalog/scout-state.json`](catalog/scout-state.json) with a reason.

Never add ports that ship game data, ROMs or ISOs, or that need them from anywhere
but the user's own copy.

## Work on the app

See [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md). Code, comments, docs and commit
messages are in English; the app's own texts are translated in
`src/i18n/locales` (English and German).
