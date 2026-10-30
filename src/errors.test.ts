import { describe, expect, it } from "vitest";
import i18n from "./i18n";
import { formatError } from "./errors";

describe("formatError", () => {
  const t = i18n.getFixedT("en");

  it("translates known codes with params", () => {
    expect(formatError({ code: "http", params: { status: 502, url: "x" } }, t)).toBe(
      "Server answered with HTTP 502.",
    );
  });

  it("computes minutes until a rate limit reset", () => {
    const resetAt = Math.floor(Date.now() / 1000) + 10 * 60;
    expect(formatError({ code: "rate_limited", params: { reset_at: resetAt } }, t)).toContain("10 minutes");
  });

  it("falls back for unknown codes and plain errors", () => {
    expect(formatError({ code: "brand_new" }, t)).toBe("Unexpected error: brand_new");
    expect(formatError(new Error("boom"), t)).toBe("Unexpected error: boom");
  });
});
