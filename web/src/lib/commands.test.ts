import { describe, expect, it } from "vitest";
import {
  COMMANDS,
  filterCommands,
  shortcutFor,
  type CommandDef,
  type CommandId,
} from "./commands";
import type { AppStore } from "@/lib/store";

// ---------------------------------------------------------------------------
// Test fixtures
// ---------------------------------------------------------------------------

/**
 * Minimal fake store — each command's `visible(store)` predicate
 * only reads a tiny slice. We keep the fixture sparse and cast to
 * `AppStore` so the test stays decoupled from the rest of the
 * slice's surface area.
 */
function fakeStore(overrides: Partial<AppStore> = {}): AppStore {
  return {
    bottomPanelMode: "default",
    isExplorerOpen: true,
    isInspectorOpen: true,
    isBottomPanelOpen: true,
    isSearchOpen: false,
    isCommandPaletteOpen: false,
    ...overrides,
  } as unknown as AppStore;
}

const echoLabel = (id: CommandId) => `label:${id}`;

describe("filterCommands", () => {
  it("returns every visible command for an empty query", () => {
    const store = fakeStore();
    const result = filterCommands(COMMANDS, store, "", echoLabel);
    // The default-mode store hides `panel-mode-default` (because
    // bottomPanelMode is already "default"). All other commands
    // should show.
    expect(result.find((c) => c.id === "panel-mode-default")).toBeUndefined();
    expect(result.find((c) => c.id === "search-entities")).toBeDefined();
    expect(result.find((c) => c.id === "navigate-design")).toBeDefined();
  });

  it("matches each query token against the localised label", () => {
    const store = fakeStore();
    const result = filterCommands(
      COMMANDS,
      store,
      "design",
      (id) =>
        id === "navigate-design"
          ? "Go to Design"
          : id === "navigate-analyze"
            ? "Go to Analyze"
            : `label:${id}`,
    );
    expect(result.map((c) => c.id)).toContain("navigate-design");
    expect(result.map((c) => c.id)).not.toContain("navigate-analyze");
  });

  it("matches against the command id as a fallback", () => {
    const store = fakeStore();
    // `toggle-explorer` is a stable id — even with a non-localised
    // label, the filter should still find it.
    const result = filterCommands(
      COMMANDS,
      store,
      "explorer",
      (id) => `label:${id}`,
    );
    expect(result.map((c) => c.id)).toContain("toggle-explorer");
  });

  it("filters out fullscreen-toggle when already in fullscreen", () => {
    const store = fakeStore({ bottomPanelMode: "fullscreen" });
    const result = filterCommands(COMMANDS, store, "", echoLabel);
    expect(result.find((c) => c.id === "panel-mode-fullscreen")).toBeUndefined();
    // And the inverse — `panel-mode-default` shows up because we're
    // not in default mode.
    expect(result.find((c) => c.id === "panel-mode-default")).toBeDefined();
  });
});

describe("shortcutFor", () => {
  const cmd: CommandDef = COMMANDS.find((c) => c.id === "search-entities")!;

  it("returns macOS hint on macOS", () => {
    expect(shortcutFor(cmd, true)).toBe("⌘K");
  });

  it("returns Ctrl hint elsewhere", () => {
    expect(shortcutFor(cmd, false)).toBe("Ctrl+K");
  });

  it("returns null when the command has no shortcut", () => {
    const navCmd = COMMANDS.find((c) => c.id === "navigate-design")!;
    expect(shortcutFor(navCmd, true)).toBeNull();
  });
});
