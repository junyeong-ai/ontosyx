import { describe, expect, it } from "vitest";

import {
  DEFAULT_LOCALE_CHAIN,
  localize,
  localizePresent,
  localizeWithFallback,
} from "./localize";

describe("localize", () => {
  it("returns the canonical default when no chain matches", () => {
    expect(localize({ default: "Customer", translations: {} }, ["ja"])).toBe(
      "Customer",
    );
  });

  it("walks the chain in order and picks the first non-empty hit", () => {
    const text = {
      default: "Customer",
      translations: { ko: "고객", en: "Customer" },
    };
    expect(localize(text, ["ko", "en"])).toBe("고객");
    expect(localize(text, ["en", "ko"])).toBe("Customer");
  });

  it("skips empty translations and falls through", () => {
    const text = {
      default: "Customer",
      translations: { ko: "", en: "Customer (en)" },
    };
    expect(localize(text, ["ko", "en"])).toBe("Customer (en)");
  });

  it("returns the canonical default when the chain is empty", () => {
    expect(localize({ default: "Customer", translations: { ko: "고객" } }, [])).toBe(
      "Customer",
    );
  });
});

describe("localizePresent", () => {
  it("returns null when the resolved value is empty", () => {
    expect(localizePresent({ default: "", translations: {} }, ["ko"])).toBeNull();
  });

  it("returns the resolved value when present", () => {
    expect(
      localizePresent(
        { default: "fallback", translations: { ko: "고객" } },
        ["ko"],
      ),
    ).toBe("고객");
  });
});

describe("localizeWithFallback", () => {
  it("uses the fallback when nothing resolves", () => {
    expect(
      localizeWithFallback({ default: "", translations: {} }, ["ko"], "Customer"),
    ).toBe("Customer");
  });

  it("uses the resolved value when present", () => {
    expect(
      localizeWithFallback(
        { default: "Customer", translations: { ko: "고객" } },
        ["ko"],
        "FALLBACK",
      ),
    ).toBe("고객");
  });
});

describe("DEFAULT_LOCALE_CHAIN", () => {
  it("matches the workspaces.admin_locale_fallback column default", () => {
    expect(DEFAULT_LOCALE_CHAIN).toEqual(["ko", "en"]);
  });
});
