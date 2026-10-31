"""Remove catalog entries by id (and from the scout's managed list).

Used by the catalog-refresh workflow for entries whose real install failed.
Usage: python -I tools/catalog-scout/drop.py <id> [<id> ...]
"""
import json
import sys

ids = set(sys.argv[1:])
for path, key in (("catalog/catalog.json", "entries"), ("catalog/scout-state.json", "managed")):
    data = json.load(open(path, encoding="utf-8"))
    if key == "entries":
        data[key] = [e for e in data[key] if e["id"] not in ids]
    else:
        data[key] = [i for i in data[key] if i not in ids]
    with open(path, "w", encoding="utf-8", newline="\n") as f:
        json.dump(data, f, indent=2, ensure_ascii=False)
        f.write("\n")
print("dropped:", ", ".join(sorted(ids)) or "nothing")
