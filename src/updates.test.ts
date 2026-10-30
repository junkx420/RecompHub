import { describe, expect, it } from "vitest";
import type { InstalledSummary } from "./api/types";
import { outdated } from "./updates";

function app(id: string, version: string): InstalledSummary {
  return {
    id,
    name: id,
    version,
    dir: `C:\\apps\\${id}`,
    exe: "game.exe",
    installed_at: "2026-01-01T00:00:00Z",
    updated_at: null,
    file_count: 1,
    previous_version: null,
  };
}

describe("outdated", () => {
  it("lists apps with a different latest tag only", () => {
    const apps = [app("a", "v1"), app("b", "v2"), app("c", "v1")];
    // "c" was never checked, "b" is current.
    expect(outdated(apps, { a: "v2", b: "v2" }).map((x) => x.id)).toEqual(["a"]);
  });

  it("treats a restored older version as outdated again", () => {
    expect(outdated([app("a", "v1")], { a: "v2" })).toHaveLength(1);
  });
});
