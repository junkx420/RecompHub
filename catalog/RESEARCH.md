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

**Release asset names were not verified against the API.** The
`verify-catalog` job in `.github/workflows/ci.yml` does that on every push
and weekly: it fetches each release and fails unless `asset_pattern`
matches exactly one asset. Fix any pattern it reports before relying on it.

| id | repo | asset name source | status |
|---|---|---|---|
| zelda64recomp | Zelda64Recomp/Zelda64Recomp | assumed from N64Recomp naming | pending API check |
| unleashedrecomp | hedge-dev/UnleashedRecomp | assumed | pending API check |
| starfox64recomp | sonicdcer/Starfox64Recomp | assumed from N64Recomp naming | pending API check |
| mariokart64recomp | sonicdcer/MarioKart64Recomp | assumed from N64Recomp naming | pending API check |
| banjorecomp | BanjoRecomp/BanjoRecomp | assumed from N64Recomp naming | pending API check |
| shipofharkinian | HarbourMasters/Shipwright | assumed (`win64`/`windows`) | pending API check |
| 2ship2harkinian | 2ship2harkinian/2ship2harkinian | assumed (`win64`/`windows`) | pending API check |
| starship | HarbourMasters/Starship | assumed (`win64`/`windows`) | pending API check |
| perfect-dark | fgsfdsfgs/perfect_dark | README links `ci-dev-build/pd-x86_64-windows.zip` | name confirmed by README |
| rmg | Rosalie241/RMG | CI artifact `RMG-Portable-Windows64-<rev>` | pending API check |

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
