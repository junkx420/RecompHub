import { describe, expect, it } from "vitest";
import de from "./locales/de.json";
import en from "./locales/en.json";
import { localized } from "./index";

function keys(obj: object, prefix = ""): string[] {
  return Object.entries(obj).flatMap(([k, v]) =>
    typeof v === "object" && v !== null ? keys(v as object, `${prefix}${k}.`) : [`${prefix}${k}`],
  );
}

describe("locales", () => {
  it("de and en define the same keys", () => {
    expect(keys(de).sort()).toEqual(keys(en).sort());
  });

  it("no empty strings", () => {
    for (const [name, locale] of Object.entries({ de, en })) {
      const empty = keys(locale).filter((k) => {
        const value = k.split(".").reduce<unknown>((o, p) => (o as Record<string, unknown>)[p], locale);
        return value === "";
      });
      expect(empty, name).toEqual([]);
    }
  });
});

describe("localized", () => {
  it("falls back to English", () => {
    expect(localized({ en: "Hello", de: "Hallo" }, "de")).toBe("Hallo");
    expect(localized({ en: "Hello" }, "de")).toBe("Hello");
    expect(localized("Plain", "de")).toBe("Plain");
    expect(localized(undefined, "de")).toBe("");
  });
});
