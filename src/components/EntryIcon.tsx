import { useState } from "react";
import type { CatalogEntry } from "../api/types";

/** Project icon, or a generated monogram if there is none or it fails to load. */
export function EntryIcon({ entry, size = 72 }: { entry: CatalogEntry; size?: number }) {
  const [failed, setFailed] = useState(false);
  const style = { width: size, height: size };
  if (entry.icon_url && !failed) {
    return (
      <img
        className="entry-icon"
        src={entry.icon_url}
        alt=""
        style={style}
        loading="lazy"
        referrerPolicy="no-referrer"
        onError={() => setFailed(true)}
      />
    );
  }
  const letters = entry.name
    .replace(/[^\p{L}\p{N} ]/gu, "")
    .split(/\s+/)
    .filter((w) => /^\p{L}/u.test(w))
    .slice(0, 2)
    .map((w) => w[0])
    .join("")
    .toUpperCase();
  return (
    <div className={`entry-icon monogram system-${entry.system}`} style={style} aria-hidden="true">
      {letters}
    </div>
  );
}
