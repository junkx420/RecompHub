# Catalog research notes

State of research for `catalog.json` (October 2026).

## How entries were checked

The development environment could not reach `api.github.com` or the
github.com web pages; only anonymous `git` reads worked. Each entry was
therefore checked against the project's repository contents (README,
release/CI workflows, source code):

- repository exists and has release tags (`git ls-remote --tags`)
- Windows executable name (from the Windows CI job)
- save/config location and portable mode (README, source)
- license file (`LICENSE`/`COPYING`)
- ROM requirements and hashes (README)

Release asset names were later checked against the API with
`verify-catalog` (30 October 2026). The `verify-catalog` job in
`.github/workflows/ci.yml` repeats that on every push and weekly: it fetches
each release and fails unless `asset_pattern` matches exactly one asset. Fix
any pattern it reports before relying on it.

| id | repo | Windows asset (30 October 2026) |
|---|---|---|
| zelda64recomp | Zelda64Recomp/Zelda64Recomp | `Zelda64Recompiled-v1.2.2-Windows.zip` |
| unleashedrecomp | hedge-dev/UnleashedRecomp | `UnleashedRecomp-Windows.zip` |
| starfox64recomp | sonicdcer/Starfox64Recomp | `Starfox64Recompiled-v1.0.3-Windows-RelWithDebInfo.zip` |
| mariokart64recomp | sonicdcer/MarioKart64Recomp | `MarioKart64Recompiled-v0.9.2-Windows-RelWithDebInfo.zip` |
| banjorecomp | BanjoRecomp/BanjoRecomp | `BanjoRecompiled-v1.0.2-Windows.zip` |
| shipofharkinian | HarbourMasters/Shipwright | `SoH-Dewey-Bravo-Win64.zip` |
| 2ship2harkinian | 2ship2harkinian/2ship2harkinian | `2Ship-Battler-Bravo-Win64.zip` |
| starship | HarbourMasters/Starship | `Starship-Barnard-Alfa-Windows.zip` |
| perfect-dark | fgsfdsfgs/perfect_dark | `pd-x86_64-windows.zip` (tag `ci-dev-build`) |
| rmg | Rosalie241/RMG | `RMG-Portable-Windows64-v0.9.0.zip` |

The sonicdcer ports add a build type (`-RelWithDebInfo`) after `-Windows`,
so their patterns allow an optional `-<word>` suffix.

## Verified details

- Zelda64Recomp saves: `%LOCALAPPDATA%\Zelda64Recompiled\saves`; portable
  mode via `portable.txt`; config files `general.json`, `graphics.json`,
  `controls.json`, `sound.json` (`src/game/config.cpp`). The other
  N64Recomp ports use the same runtime template; their config names are
  taken from Zelda64Recomp.
- Unleashed Recompiled installs the user's game files into `game/`, `dlc/`,
  `update/` and `patched/` next to the executable
  (`UnleashedRecomp/install/installer.cpp`), so these are in `preserve`.
  User data: `%APPDATA%\UnleashedRecomp\` (README).
- Ship of Harkinian: `shipofharkinian.json`, `oot.o2r`, `oot-mq.o2r`
  (README). 2 Ship 2 Harkinian: `2ship2harkinian.json`. Starship:
  `starship.cfg.json`, `sf64.o2r`. A blanket `*.o2r` is avoided because the
  ports ship their own `.o2r` that must update.
- Perfect Dark ROM MD5s are copied from the project README. The project
  publishes MD5 only, so `accepted_hashes` uses `md5`.
- Files that are not part of an installed release (created at runtime by
  the port or the user) are always kept on update, independent of `preserve`.

## Not included

| project | reason |
|---|---|
| DinosaurPlanetRecomp/dino-recomp | The game was never released; a "user's own copy" does not exist legally. |
| klorfmorf/Goemon64Recomp | Only pre-release tags (`v0.2.0-dev`); `releases/latest` excludes them. |
| coop-deluxe/sm64coopdx | No license file found; Windows asset name unverified. Candidate for later. |
| HarbourMasters/SpaghettiKart, HarbourMasters/Ghostship | Windows asset names unverified; candidates for later. |
| xenia-canary, emulators without GitHub releases (Dolphin) | Releases elsewhere or rolling-tag only. |
| Tools category | No tool with a stable Windows release asset was found yet. |

`license: NOASSERTION` marks projects without a license file
(Ship of Harkinian).
