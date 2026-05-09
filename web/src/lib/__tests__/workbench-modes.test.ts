import { beforeEach, describe, expect, it } from "vitest";
import {
  _resetWorkbenchModeRegistryForTests,
  listModesByCategory,
  listWorkbenchModes,
  registerWorkbenchMode,
  unregisterWorkbenchMode,
  workbenchModeById,
  type WorkbenchMode,
} from "../workbench-modes";
import { Settings2 } from "lucide-react";
const PLUGIN_MODE: WorkbenchMode = {
  id: "ext-test-mode",
  labelKey: "ext-test-mode",
  icon: Settings2,
  href: "/ext-test-mode",
  // No `shortcut` — plugin modes may omit it.
};

describe("workbench-mode registry (defaults)", () => {
  beforeEach(() => {
    _resetWorkbenchModeRegistryForTests();
  });

  it("exposes a non-empty registry of modes", () => {
    expect(listWorkbenchModes().length).toBeGreaterThan(0);
  });

  it("default workbench entries carry an `href` and a navigation shortcut", () => {
    // Workbench modes are derived from `defaultMode()` which pins
    // `href = "/<id>"` and a navigation shortcut. Operations modes
    // route under `/settings/*` and ship without shortcuts; they
    // are exercised by the operations-category test below.
    for (const mode of listModesByCategory("workbench")) {
      expect(mode.href).toBe(`/${mode.id}`);
      expect(mode.shortcut?.route).toBe(mode.id);
    }
  });

  it("operations entries route under settings and omit shortcuts", () => {
    const ops = listModesByCategory("operations");
    expect(ops.length).toBeGreaterThan(0);
    for (const mode of ops) {
      expect(mode.href.startsWith("/settings/")).toBe(true);
      expect(mode.shortcut).toBeUndefined();
    }
  });

  it("only `design` opts into panel toggles by default", () => {
    const withToggles = listWorkbenchModes().filter((m) => m.hasPanelToggles);
    expect(withToggles.map((m) => m.id)).toEqual(["design"]);
  });

  it("entries are unique by id", () => {
    const ids = listWorkbenchModes().map((m) => m.id);
    expect(new Set(ids).size).toBe(ids.length);
  });
});

describe("workbenchModeById", () => {
  beforeEach(() => {
    _resetWorkbenchModeRegistryForTests();
  });

  it("looks modes up by id", () => {
    expect(workbenchModeById("design")?.id).toBe("design");
    expect(workbenchModeById("recipes")?.id).toBe("recipes");
  });

  it("returns undefined for unknown ids", () => {
    expect(workbenchModeById("ext-no-such-mode")).toBeUndefined();
  });
});

describe("registerWorkbenchMode / unregisterWorkbenchMode", () => {
  beforeEach(() => {
    _resetWorkbenchModeRegistryForTests();
  });

  it("appends a fresh plugin mode at the end by default", () => {
    registerWorkbenchMode(PLUGIN_MODE);
    const ids = listWorkbenchModes().map((m) => m.id);
    expect(ids[ids.length - 1]).toBe(PLUGIN_MODE.id);
  });

  it("`before` inserts ahead of the named mode", () => {
    registerWorkbenchMode(PLUGIN_MODE, { before: "analyze" });
    const ids = listWorkbenchModes().map((m) => m.id);
    expect(ids.indexOf(PLUGIN_MODE.id)).toBe(ids.indexOf("analyze") - 1);
  });

  it("`after` inserts following the named mode", () => {
    registerWorkbenchMode(PLUGIN_MODE, { after: "design" });
    const ids = listWorkbenchModes().map((m) => m.id);
    expect(ids.indexOf(PLUGIN_MODE.id)).toBe(ids.indexOf("design") + 1);
  });

  it("re-registering preserves position and replaces fields", () => {
    const before = listWorkbenchModes().map((m) => m.id);
    registerWorkbenchMode({
      ...workbenchModeById("design")!,
      icon: Settings2,
    });
    const after = listWorkbenchModes().map((m) => m.id);
    expect(after).toEqual(before);
    expect(workbenchModeById("design")?.icon).toBe(Settings2);
  });

  it("unregister removes the mode and is idempotent", () => {
    registerWorkbenchMode(PLUGIN_MODE);
    unregisterWorkbenchMode(PLUGIN_MODE.id);
    expect(workbenchModeById(PLUGIN_MODE.id)).toBeUndefined();
    expect(() => unregisterWorkbenchMode(PLUGIN_MODE.id)).not.toThrow();
  });
});
