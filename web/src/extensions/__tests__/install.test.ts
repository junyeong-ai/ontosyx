import { describe, it, expect, beforeEach, vi } from "vitest";
import { Book02Icon } from "@hugeicons/core-free-icons";

import {
  _resetInspectorFacetRegistryForTests,
  listInspectorFacets,
  type InspectorFacet,
} from "@/components/workbench/inspector/facets/registry";
import {
  listWorkbenchModes,
  type WorkbenchMode,
} from "@/lib/workbench-modes";

import {
  _uninstallAllExtensionsForTests,
  installExtensions,
  listInstalledExtensionIds,
} from "../install";
import type { WorkbenchExtension } from "../types";

const TEST_FACET: InspectorFacet = {
  id: "ext-test-facet",
  labelKey: "ext-test-facet",
  accept: () => true,
  render: () => null,
};

const TEST_FACET_2: InspectorFacet = {
  id: "ext-test-facet-2",
  labelKey: "ext-test-facet-2",
  accept: () => true,
  render: () => null,
};

// Capture the default `design` mode so the replace-round-trip test
// can assert the original returns after uninstall. Captured at
// module load so the assertion is independent of registration
// order across the default set.
const ORIGINAL_DESIGN: WorkbenchMode = listWorkbenchModes().find(
  (m) => m.id === "design",
)!;

const REPLACEMENT_DESIGN: WorkbenchMode = {
  ...ORIGINAL_DESIGN,
  labelKey: "ext-test-replaced-design",
  icon: Book02Icon,
};

// Plugin-style new mode — exercises the open `WorkspaceMode` path
// where an extension registers an id outside the default seven.
const PLUGIN_AUDIT_MODE: WorkbenchMode = {
  id: "ext-audit",
  labelKey: "ext-audit",
  icon: Book02Icon,
  href: "/ext-audit",
  // No `shortcut` — plugin modes may omit the g-prefix shortcut.
};

describe("installExtensions", () => {
  beforeEach(() => {
    _uninstallAllExtensionsForTests();
    _resetInspectorFacetRegistryForTests();
  });

  it("installs each extension once and reports the installed ids", () => {
    const ext: WorkbenchExtension = {
      id: "alpha",
      install: (api) => {
        api.registerInspectorFacet(TEST_FACET);
        return () => api.unregisterInspectorFacet(TEST_FACET.id);
      },
    };
    const seen = installExtensions([ext]);
    expect(seen).toEqual(["alpha"]);
    expect(listInstalledExtensionIds()).toEqual(["alpha"]);
    expect(listInspectorFacets().some((f) => f.id === TEST_FACET.id)).toBe(
      true,
    );
  });

  it("uninstall reverses inspector facet registrations symmetrically", () => {
    const ext: WorkbenchExtension = {
      id: "beta",
      install: (api) => {
        api.registerInspectorFacet(TEST_FACET);
        return () => api.unregisterInspectorFacet(TEST_FACET.id);
      },
    };
    installExtensions([ext]);
    expect(listInspectorFacets().some((f) => f.id === TEST_FACET.id)).toBe(
      true,
    );
    installExtensions([]);
    expect(listInspectorFacets().some((f) => f.id === TEST_FACET.id)).toBe(
      false,
    );
  });

  it("workbench-mode plugin id round-trips: install adds, uninstall removes", () => {
    const ext: WorkbenchExtension = {
      id: "audit-plugin",
      install: (api) => {
        api.registerWorkbenchMode(PLUGIN_AUDIT_MODE);
        return () => api.unregisterWorkbenchMode(PLUGIN_AUDIT_MODE.id);
      },
    };
    installExtensions([ext]);
    expect(
      listWorkbenchModes().some((m) => m.id === PLUGIN_AUDIT_MODE.id),
    ).toBe(true);
    installExtensions([]);
    expect(
      listWorkbenchModes().some((m) => m.id === PLUGIN_AUDIT_MODE.id),
    ).toBe(false);
  });

  it("workbench-mode replacement round-trips: install swaps, uninstall restores", () => {
    const ext: WorkbenchExtension = {
      id: "mode-swap",
      install: (api) => {
        api.registerWorkbenchMode(REPLACEMENT_DESIGN);
        return () => api.registerWorkbenchMode(ORIGINAL_DESIGN);
      },
    };
    installExtensions([ext]);
    const swapped = listWorkbenchModes().find((m) => m.id === "design")!;
    expect(swapped.labelKey).toBe(REPLACEMENT_DESIGN.labelKey);

    installExtensions([]);
    const restored = listWorkbenchModes().find((m) => m.id === "design")!;
    expect(restored.labelKey).toBe(ORIGINAL_DESIGN.labelKey);
  });

  it("re-installing the same id runs the prior uninstall first", () => {
    const uninstall = vi.fn();
    const ext: WorkbenchExtension = {
      id: "gamma",
      install: () => uninstall,
    };
    installExtensions([ext]);
    installExtensions([ext]);
    expect(uninstall).toHaveBeenCalledTimes(1);
  });

  it("dropping an extension on the next install runs its uninstall", () => {
    const uninstall = vi.fn();
    const ext: WorkbenchExtension = {
      id: "delta",
      install: () => uninstall,
    };
    installExtensions([ext]);
    installExtensions([]);
    expect(uninstall).toHaveBeenCalledTimes(1);
    expect(listInstalledExtensionIds()).toEqual([]);
  });

  it("duplicate ids in the same boot pass keep the first registration", () => {
    const installA = vi.fn(() => () => {});
    const installB = vi.fn(() => () => {});
    const a: WorkbenchExtension = { id: "epsilon", install: installA };
    const b: WorkbenchExtension = { id: "epsilon", install: installB };

    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    const seen = installExtensions([a, b]);
    expect(seen).toEqual(["epsilon"]);
    expect(installA).toHaveBeenCalledTimes(1);
    expect(installB).not.toHaveBeenCalled();
    expect(warn).toHaveBeenCalled();
    warn.mockRestore();
  });

  it("an uninstall that throws does not prevent subsequent uninstalls", () => {
    const goodUninstall = vi.fn();
    const ext1: WorkbenchExtension = {
      id: "zeta",
      install: () => () => {
        throw new Error("boom");
      },
    };
    const ext2: WorkbenchExtension = {
      id: "eta",
      install: () => goodUninstall,
    };
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    installExtensions([ext1, ext2]);
    installExtensions([]);
    expect(goodUninstall).toHaveBeenCalledTimes(1);
    expect(listInstalledExtensionIds()).toEqual([]);
    warn.mockRestore();
  });

  it("two extensions registering distinct facets both land", () => {
    const ext1: WorkbenchExtension = {
      id: "theta",
      install: (api) => {
        api.registerInspectorFacet(TEST_FACET);
        return () => api.unregisterInspectorFacet(TEST_FACET.id);
      },
    };
    const ext2: WorkbenchExtension = {
      id: "iota",
      install: (api) => {
        api.registerInspectorFacet(TEST_FACET_2);
        return () => api.unregisterInspectorFacet(TEST_FACET_2.id);
      },
    };
    installExtensions([ext1, ext2]);
    const ids = listInspectorFacets().map((f) => f.id);
    expect(ids).toContain(TEST_FACET.id);
    expect(ids).toContain(TEST_FACET_2.id);
  });
});
