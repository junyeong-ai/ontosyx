import { describe, expect, it, beforeEach, vi } from "vitest";
import {
  eventMatchesCombo,
  formatGlyph,
  normalizeCombo,
  parseCombo,
} from "../combo";

function pinPlatform(platform: string): void {
  Object.defineProperty(navigator, "platform", {
    value: platform,
    configurable: true,
  });
}

beforeEach(() => {
  pinPlatform("Win32");
});

describe("parseCombo", () => {
  it("parses a bare key as zero modifiers", () => {
    const c = parseCombo("a");
    expect(c.modifiers.size).toBe(0);
    expect(c.key).toBe("a");
  });

  it("lowercases letter keys but preserves special-key case", () => {
    expect(parseCombo("A").key).toBe("a");
    expect(parseCombo("Escape").key).toBe("Escape");
    expect(parseCombo("ArrowDown").key).toBe("ArrowDown");
  });

  it("collects modifiers", () => {
    const c = parseCombo("ctrl+shift+s");
    expect(c.modifiers.has("ctrl")).toBe(true);
    expect(c.modifiers.has("shift")).toBe(true);
    expect(c.key).toBe("s");
  });

  it("resolves `mod` to ctrl on non-mac platforms", () => {
    pinPlatform("Win32");
    expect(parseCombo("mod+k").modifiers.has("ctrl")).toBe(true);
    expect(parseCombo("mod+k").modifiers.has("meta")).toBe(false);
  });

  it("resolves `mod` to meta on macOS", () => {
    pinPlatform("MacIntel");
    expect(parseCombo("mod+k").modifiers.has("meta")).toBe(true);
    expect(parseCombo("mod+k").modifiers.has("ctrl")).toBe(false);
  });

  it("rejects an unknown modifier token", () => {
    expect(() => parseCombo("super+x")).toThrow(/super/);
  });

  it("rejects an empty combo", () => {
    expect(() => parseCombo("")).toThrow();
  });
});

describe("normalizeCombo", () => {
  it("orders modifiers consistently regardless of input order", () => {
    pinPlatform("Win32");
    expect(normalizeCombo("shift+ctrl+s")).toBe(
      normalizeCombo("ctrl+shift+s"),
    );
  });

  it("normalises `mod` per platform — same combo on different OSes hashes differently", () => {
    pinPlatform("MacIntel");
    const onMac = normalizeCombo("mod+k");
    pinPlatform("Win32");
    const onWin = normalizeCombo("mod+k");
    expect(onMac).not.toBe(onWin);
    expect(onMac).toContain("meta");
    expect(onWin).toContain("ctrl");
  });
});

describe("eventMatchesCombo", () => {
  function ev(init: KeyboardEventInit): KeyboardEvent {
    return new KeyboardEvent("keydown", init);
  }

  it("matches a bare letter without modifiers", () => {
    expect(eventMatchesCombo(ev({ key: "a" }), "a")).toBe(true);
    expect(eventMatchesCombo(ev({ key: "b" }), "a")).toBe(false);
  });

  it("respects shift", () => {
    expect(
      eventMatchesCombo(ev({ key: "a", shiftKey: true }), "shift+a"),
    ).toBe(true);
    expect(eventMatchesCombo(ev({ key: "a" }), "shift+a")).toBe(false);
    expect(
      eventMatchesCombo(ev({ key: "a", shiftKey: true }), "a"),
    ).toBe(false);
  });

  it("matches `mod` against the platform's primary modifier", () => {
    pinPlatform("MacIntel");
    expect(
      eventMatchesCombo(ev({ key: "k", metaKey: true }), "mod+k"),
    ).toBe(true);
    expect(
      eventMatchesCombo(ev({ key: "k", ctrlKey: true }), "mod+k"),
    ).toBe(false);
    pinPlatform("Win32");
    expect(
      eventMatchesCombo(ev({ key: "k", ctrlKey: true }), "mod+k"),
    ).toBe(true);
    expect(
      eventMatchesCombo(ev({ key: "k", metaKey: true }), "mod+k"),
    ).toBe(false);
  });

  it("matches special keys exactly", () => {
    expect(eventMatchesCombo(ev({ key: "Escape" }), "Escape")).toBe(true);
    expect(eventMatchesCombo(ev({ key: "escape" }), "Escape")).toBe(false);
  });
});

describe("formatGlyph", () => {
  it("renders ⌘ on macOS for `mod`", () => {
    pinPlatform("MacIntel");
    expect(formatGlyph("mod+k")).toContain("⌘");
    expect(formatGlyph("mod+k")).toContain("K");
  });

  it("renders Ctrl+ on non-mac for `mod`", () => {
    pinPlatform("Win32");
    expect(formatGlyph("mod+k")).toContain("Ctrl+");
  });

  it("uses the special-key glyph table for arrows / escape", () => {
    expect(formatGlyph("Escape")).toBe("Esc");
    expect(formatGlyph("ArrowDown")).toBe("↓");
  });
});

// Silence the user-agent platform spy at the end of the suite so
// other test files don't see the patched value.
vi.unstubAllGlobals?.();
