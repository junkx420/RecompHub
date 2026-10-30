import { describe, expect, it } from "vitest";
import { formatBytes } from "./format";

describe("formatBytes", () => {
  it("uses binary units", () => {
    expect(formatBytes(0, "en")).toBe("0 B");
    expect(formatBytes(1023, "en")).toBe("1,023 B");
    expect(formatBytes(1536, "en")).toBe("1.5 KB");
    expect(formatBytes(48 * 1024 * 1024, "en")).toBe("48.0 MB");
  });

  it("follows the locale", () => {
    expect(formatBytes(1536, "de")).toBe("1,5 KB");
  });
});
