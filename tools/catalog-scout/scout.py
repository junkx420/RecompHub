"""Keep catalog/catalog.json up to date with recomp and decomp ports.

Source: the rated, CC0 data of github.com/SubmitGame/awesome-decomp-games.

What a run does:
  1. Entries listed in catalog/scout-state.json ("managed") are refreshed:
     a broken asset pattern is rebuilt from the current release, a port that
     reached a stable release drops `include_prereleases`, and an
     experimental port that now qualifies as playable is promoted (never
     demoted automatically).
  2. New candidates (not in the catalog, not excluded) are added when their
     release has a Windows zip/7z/exe whose executable can be found. For
     zips only the central directory is read (HTTP range requests).
  3. Writes the catalog and state, a Markdown summary, and the ids of new
     or changed entries (for the live install check).

Nothing is downloaded in full and nothing is executed here; the workflow
installs changed entries afterwards with the `live_install` test.

Usage: python -I tools/catalog-scout/scout.py [--min-stars N] [--summary F] [--changed F]
Needs GITHUB_TOKEN.
"""
import argparse
import json
import os
import re
import struct
import urllib.error
import urllib.request

LIST_URL = "https://raw.githubusercontent.com/SubmitGame/awesome-decomp-games/main/data/games.json"
TOKEN = os.environ.get("GITHUB_TOKEN", "")

NOT_CONSOLE = {"PC", "DOS", "Mac", "Mobile", "Amiga", "Commodore 64/8-bit", "Others", "Other"}
BAD_WORDS = {"linux", "mac", "macos", "osx", "universal", "darwin", "android", "apk", "ios", "ipa",
             "arm64", "aarch64", "switch", "nx", "3ds", "vita", "psp", "ps4", "ps5", "debug", "symbol",
             "symbols", "pdb", "pdbs", "source", "src", "appimage", "flatpak", "deb", "rpm", "dmg",
             "steamdeck", "wasm", "web"}
EXE_JUNK = re.compile(r"(unins|vc_?redist|crash|dxsetup|setup|helper|7z|reg|dump|installer|update|"
                      r"cmake|test|ffmpeg|python|elevat|uninstall)", re.I)
# Executables that build or patch something instead of being the game.
TOOL_EXE = re.compile(r"(builder|maker|porter|patcher|compiler|cc1|rexglue|sdk)", re.I)
SYSTEMS = {
    "N64": "n64", "Xbox 360": "x360", "og xbox": "xbox", "Xbox": "xbox", "PS1": "ps1", "PS2": "ps2",
    "PS3": "ps3", "PlayStation 4": "ps4", "PS4": "ps4", "Wii": "wii", "Wii U": "wiiu", "GameCube": "gc",
    "GBA": "gba", "Game Boy": "gb", "Game Boy Color": "gbc", "SNES": "snes", "NES": "nes", "DOS": "dos",
    "Arcade": "arcade", "PC": "pc", "Sega CD/32X": "segacd", "PSP": "psp", "DS": "nds", "Genesis": "md",
    "Dreamcast": "dc", "Mobile": "mobile",
}
VERSIONISH = re.compile(
    r"v?\d+(?:[._-]\d+)*(?:[-+._]?(?:alpha|beta|rc|dev|b|g[0-9a-f]{7,})\.?\d*)*|[0-9a-f]{7,40}", re.I)
SLOT = r"(?:v?[0-9][0-9._+-]*(?:(?:alpha|beta|rc|dev|b)[0-9._-]*)?(?:\+?g[0-9a-f]{7,})?|[0-9a-f]{7,40})"
PRE_WORDS = re.compile(r"(alpha|beta|dev|rc\d*|wip|preview|test|nightly)", re.I)
HINT = {
    "en": "Needs files from your own copy of the game. The port asks for them on first start or "
          "explains where to put them; the project page says which version it supports.",
    "de": "Braucht Dateien aus deiner eigenen Kopie des Spiels. Der Port fragt beim ersten Start danach "
          "oder erklärt, wohin sie gehören; auf der Projektseite steht, welche Version unterstützt wird.",
}


def http(url, headers=None):
    req = urllib.request.Request(url, headers={"User-Agent": "RecompHub-catalog-scout", **(headers or {})})
    return urllib.request.urlopen(req, timeout=60)


def api(path):
    with http(f"https://api.github.com/repos/{path}", {
            "Authorization": f"Bearer {TOKEN}", "Accept": "application/vnd.github+json"}) as r:
        return json.load(r)


def fetch_range(url, start, end):
    with http(url, {"Range": f"bytes={start}-{end}"}) as r:
        return r.read()


def zip_names(url, size):
    """Names in a zip's central directory, read with range requests."""
    tail_len = min(size, 66000)
    tail = fetch_range(url, size - tail_len, size - 1)
    i = tail.rfind(b"PK\x05\x06")
    if i < 0:
        return None
    cd_size, cd_offset = struct.unpack("<II", tail[i + 12:i + 20])
    if cd_offset == 0xFFFFFFFF:
        j = tail.rfind(b"PK\x06\x06")
        if j < 0:
            return None
        cd_size, cd_offset = struct.unpack("<QQ", tail[j + 40:j + 56])
    cd = fetch_range(url, cd_offset, cd_offset + cd_size - 1)
    names, p = [], 0
    while p + 46 <= len(cd) and cd[p:p + 4] == b"PK\x01\x02":
        usize = struct.unpack("<I", cd[p + 24:p + 28])[0]
        nlen, elen, clen = struct.unpack("<HHH", cd[p + 28:p + 34])
        names.append((cd[p + 46:p + 46 + nlen].decode("utf-8", "replace").replace("\\", "/"), usize))
        p += 46 + nlen + elen + clen
    return names


def latest_release(repo, allow_pre):
    """(release, is_prerelease) or (None, False)."""
    try:
        return api(f"{repo}/releases/latest"), False
    except urllib.error.HTTPError as e:
        if e.code != 404:
            raise
    if not allow_pre:
        return None, False
    rels = [r for r in api(f"{repo}/releases?per_page=10") if not r["draft"]]
    return (rels[0], True) if rels else (None, False)


def windows_assets(assets):
    ranked = []
    for a in assets:
        n = a["name"].lower()
        if not n.endswith((".zip", ".7z", ".exe")):
            continue
        words = set(re.split(r"[^a-z0-9]+", n))
        if words & BAD_WORDS:
            continue
        if n.endswith(".exe") and ("setup" in n or "install" in n):
            continue  # installers cannot be unpacked
        score = 2 if words & {"windows", "win64", "win", "win32", "x64", "x86", "pc", "msvc"} else 0
        score += 1 if ("x64" in n or "win64" in n) else 0
        ranked.append((score, a))
    ranked.sort(key=lambda t: -t[0])
    return [a for _, a in ranked]


def pick_exe(names, repo):
    files = [(n, s) for n, s in names if not n.endswith("/")]
    exes = [(n, s) for n, s in files if n.lower().endswith(".exe")]
    good = [(n, s) for n, s in exes if not EXE_JUNK.search(n.rsplit("/", 1)[-1])]
    pool = good or exes
    if not pool:
        return None
    tokens = set(re.split(r"[^a-z0-9]+", repo.lower())) - {"", "recomp", "recompiled", "pc", "port"}

    def rank(item):
        name = item[0].rsplit("/", 1)[-1].lower()
        return (-sum(1 for t in tokens if len(t) > 2 and t in name), item[0].count("/"), -item[1])

    exe = sorted(pool, key=rank)[0][0]
    # RecompHub flattens a single top folder; express the path below it.
    tops = {n.split("/", 1)[0] for n, _ in files}
    if len(tops) == 1 and "/" in exe:
        exe = exe.split("/", 1)[1]
    return exe


def pattern_for(name, all_names):
    parts, last = [], 0
    for m in VERSIONISH.finditer(name):
        before = name[m.start() - 1] if m.start() else ""
        if before.isalpha() and not m.group().lower().startswith("v"):
            continue
        parts += [re.escape(name[last:m.start()]), SLOT]
        last = m.end()
    parts.append(re.escape(name[last:]))
    pattern = "(?i)^" + "".join(parts).replace(r"\-", "-").replace(r"\ ", " ") + "$"
    if [n for n in all_names if re.search(pattern, n)] == [name]:
        return pattern
    return "(?i)^" + re.escape(name).replace(r"\-", "-").replace(r"\ ", " ") + "$"


def version_tuple(tag):
    m = re.search(r"(\d+)\.(\d+)", tag or "")
    return (int(m.group(1)), int(m.group(2))) if m else None


def is_playable(tag, prerelease, compl, stars):
    if prerelease or PRE_WORDS.search(tag or ""):
        return False
    ver = version_tuple(tag)
    return (ver is not None and ver[0] >= 1) or (compl >= 4 and stars >= 300)


def scout(repo):
    """Release facts for a repo, or (None, reason)."""
    rel, pre = latest_release(repo, allow_pre=True)
    if not rel:
        return None, "no release"
    assets = windows_assets(rel["assets"])
    if not assets:
        return None, "no Windows asset"
    asset = assets[0]
    name = asset["name"]
    if name.lower().endswith(".zip"):
        names = zip_names(asset["browser_download_url"], asset["size"])
        exe = pick_exe(names, repo) if names else None
    elif name.lower().endswith(".exe"):
        exe = name
    else:
        return None, "7z asset (needs a manual entry)"
    if not exe:
        return None, "no executable found"
    if TOOL_EXE.search(exe.rsplit("/", 1)[-1]):
        return None, f"executable looks like a tool: {exe}"
    return {
        "tag": rel["tag_name"], "prerelease": pre, "asset": name,
        "assets": [a["name"] for a in rel["assets"]], "exe": exe,
    }, None


def slug(repo, taken):
    base = re.sub(r"[^a-z0-9]+", "-", repo.split("/")[1].lower()).strip("-")[:60] or "port"
    eid, n = base, 2
    while eid in taken:
        eid, n = f"{base}-{n}", n + 1
    return eid


_canonical = {}


def canonical(repo):
    """owner/name as GitHub reports it now (follows renames and transfers)."""
    key = repo.lower()
    if key not in _canonical:
        try:
            _canonical[key] = api(repo)["full_name"].lower()
        except Exception:  # noqa: BLE001 - deleted or private repos keep their name
            _canonical[key] = key
    return _canonical[key]


def candidates(games, min_stars):
    for g in games:
        if not g.get("repo") or g.get("archived") or g.get("inactive") or not g.get("link_ok"):
            continue
        if g.get("ai_flag") or not g.get("releases") or g.get("stars", 0) < min_stars:
            continue
        if g["type"] == "static-recomp" or (g["type"] == "source-port" and g["platform"] not in NOT_CONSOLE):
            yield g


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--catalog", default="catalog/catalog.json")
    ap.add_argument("--state", default="catalog/scout-state.json")
    ap.add_argument("--min-stars", type=int, default=50)
    ap.add_argument("--summary", default=None)
    ap.add_argument("--changed", default=None)
    args = ap.parse_args()

    catalog = json.load(open(args.catalog, encoding="utf-8"))
    state = json.load(open(args.state, encoding="utf-8"))
    excluded = {k.lower(): v for k, v in state["excluded"].items()}
    managed = set(state["managed"])
    with http(LIST_URL) as r:
        games = json.load(r)["games"]
    by_repo = {g["repo"].lower(): g for g in games if g.get("repo")}
    # Compare by current repository name: projects get renamed or moved.
    in_catalog = {canonical(e["repo"]) for e in catalog["entries"]}
    ids = {e["id"] for e in catalog["entries"]}
    added, changed, skipped = [], [], []

    # 1. Refresh managed entries.
    for e in catalog["entries"]:
        if e["id"] not in managed:
            continue
        g = by_repo.get(e["repo"].lower(), {})
        try:
            facts, why = scout(e["repo"])
        except Exception as err:  # noqa: BLE001 - network hiccups must not stop the run
            skipped.append((e["repo"], f"error: {err}"))
            continue
        if not facts:
            skipped.append((e["repo"], f"kept as is: {why}"))
            continue
        notes = []
        # Only a pattern that no longer matches exactly one asset is rebuilt.
        if sum(1 for n in facts["assets"] if re.search(e["asset_pattern"], n)) != 1:
            e["asset_pattern"] = pattern_for(facts["asset"], facts["assets"])
            e["exe"] = facts["exe"]
            notes.append("asset pattern rebuilt")
        if e.get("include_prereleases") and not facts["prerelease"]:
            del e["include_prereleases"]
            notes.append("stable release available")
        if e.get("maturity") == "experimental" and is_playable(
                facts["tag"], facts["prerelease"], g.get("ratings", {}).get("completeness", 0), g.get("stars", 0)):
            e["maturity"] = "playable"
            notes.append(f"promoted to playable ({facts['tag']})")
        if notes:
            changed.append((e["id"], ", ".join(notes)))

    # 2. Add new candidates.
    for g in sorted(candidates(games, args.min_stars), key=lambda g: -g["stars"]):
        key = g["repo"].lower()
        if key in excluded:
            continue
        try:
            key = canonical(g["repo"])
            if key in in_catalog or key in excluded:
                continue
            facts, why = scout(g["repo"])
        except Exception as err:  # noqa: BLE001
            skipped.append((g["repo"], f"error: {err}"))
            continue
        if not facts:
            skipped.append((g["repo"], why))
            continue
        eid = slug(g["repo"], ids)
        ids.add(eid)
        entry = {
            "id": eid, "name": g["title"], "category": "game",
            "system": SYSTEMS.get(g["platform"], "other"), "repo": g["repo"],
            "asset_pattern": pattern_for(facts["asset"], facts["assets"]), "exe": facts["exe"],
        }
        if facts["prerelease"]:
            entry["include_prereleases"] = True
        entry["maturity"] = "playable" if is_playable(
            facts["tag"], facts["prerelease"], g["ratings"]["completeness"], g["stars"]) else "experimental"
        entry["preserve"] = ["portable.txt", "saves/**", "mods/**"]
        entry["game_data"] = {"mode": "port_handles_it", "hint": HINT}
        entry["description"] = {"en": (g.get("note") or g.get("description") or g["title"]).strip()}
        entry["credits"] = [g["repo"].split("/")[0]]
        entry["license"] = g.get("license") or "NOASSERTION"
        entry["homepage"] = f"https://github.com/{g['repo']}"
        catalog["entries"].append(entry)
        managed.add(eid)
        in_catalog.add(key)
        added.append((eid, f"{g['title']} ({entry['maturity']}, {g['stars']} stars, {facts['tag']})"))

    state["managed"] = sorted(managed)
    for path, data in ((args.catalog, catalog), (args.state, state)):
        with open(path, "w", encoding="utf-8", newline="\n") as f:
            json.dump(data, f, indent=2, ensure_ascii=False)
            f.write("\n")

    lines = [f"## Catalog refresh", "", f"{len(added)} added, {len(changed)} changed.", ""]
    if added:
        lines += ["### Added", ""] + [f"- `{i}`: {t}" for i, t in added] + [""]
    if changed:
        lines += ["### Changed", ""] + [f"- `{i}`: {t}" for i, t in changed] + [""]
    if skipped:
        lines += ["<details><summary>Skipped</summary>", ""] + \
                 [f"- {r}: {w}" for r, w in skipped] + ["", "</details>", ""]
    summary = "\n".join(lines)
    print(summary)
    if args.summary:
        open(args.summary, "w", encoding="utf-8").write(summary)
    if args.changed:
        open(args.changed, "w", encoding="utf-8").write(
            "\n".join(i for i, _ in added + changed) + ("\n" if added or changed else ""))


if __name__ == "__main__":
    main()
