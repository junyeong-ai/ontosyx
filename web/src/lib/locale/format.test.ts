import { describe, it, expect } from "vitest";

import {
  DATE_PRESETS,
  formatDate,
  formatNumber,
  formatRelativeTime,
} from "./format";

describe("formatNumber", () => {
  it("renders thousand separators per locale", () => {
    expect(formatNumber(1234567, ["ko"])).toBe("1,234,567");
    expect(formatNumber(1234567, ["en"])).toBe("1,234,567");
    expect(formatNumber(1234567, ["de"])).toBe("1.234.567");
  });

  it("walks the chain when the leading tag isn't recognised", () => {
    expect(formatNumber(1500, ["zz-XX", "de"])).toBe("1.500");
  });

  it("respects Intl options pass-through", () => {
    expect(formatNumber(0.42, ["en"], { style: "percent" })).toBe("42%");
    expect(
      formatNumber(1500, ["ko"], { style: "currency", currency: "KRW" }),
    ).toBe("₩1,500");
  });
});

describe("formatDate", () => {
  const fixed = new Date("2026-05-03T10:30:00Z");

  it("uses the dateTime preset by default", () => {
    const out = formatDate(fixed, ["en"]);
    // Output varies with timezone — just assert that the year + a month
    // token appear so we don't pin the test to the runner's TZ.
    expect(out).toMatch(/2026/);
    expect(out).toMatch(/May/);
  });

  it("accepts ISO string and epoch ms inputs", () => {
    const a = formatDate("2026-05-03T10:30:00Z", ["en"]);
    const b = formatDate(fixed.getTime(), ["en"]);
    expect(a).toBe(b);
  });

  it("renders Korean dates against the ko locale", () => {
    // Korean medium-style includes year + month + day. ICU localises
    // separators / suffixes per release; we assert the year and a month
    // marker (either "5월" verbose or "5." numeric, depending on style).
    const longOut = formatDate(fixed, ["ko"], { year: "numeric", month: "long", day: "numeric" });
    expect(longOut).toMatch(/2026/);
    expect(longOut).toMatch(/5월/);
  });

  it("renders an isoDate preset that contains year, month, and day numbers", () => {
    const out = formatDate(fixed, ["en"], DATE_PRESETS.isoDate);
    expect(out).toMatch(/2026/);
    expect(out).toMatch(/05/);
    expect(out).toMatch(/03/);
  });
});

describe("formatRelativeTime", () => {
  const now = new Date("2026-05-03T10:00:00Z");

  it("picks the largest unit that fits", () => {
    const tenMinAgo = new Date("2026-05-03T09:50:00Z");
    expect(formatRelativeTime(tenMinAgo, ["en"], now)).toBe("10 minutes ago");
  });

  it("emits future phrases for upcoming times", () => {
    const inTwoHours = new Date("2026-05-03T12:00:00Z");
    expect(formatRelativeTime(inTwoHours, ["en"], now)).toBe("in 2 hours");
  });

  it("uses 'auto' so 1-day-ago renders as 'yesterday'", () => {
    const yesterday = new Date("2026-05-02T10:00:00Z");
    expect(formatRelativeTime(yesterday, ["en"], now)).toBe("yesterday");
  });

  it("renders Korean relative phrases against the ko locale", () => {
    const fiveMinAgo = new Date("2026-05-03T09:55:00Z");
    expect(formatRelativeTime(fiveMinAgo, ["ko"], now)).toBe("5분 전");
  });
});

describe("compact notation", () => {
  it("emits locale-correct abbreviations via Intl notation:compact", () => {
    const opts: Intl.NumberFormatOptions = {
      notation: "compact",
      maximumFractionDigits: 1,
    };
    expect(formatNumber(12345, ["en"], opts)).toBe("12.3K");
    expect(formatNumber(12345, ["ko"], opts)).toBe("1.2만");
  });
});
