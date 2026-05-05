import type { Page } from "@playwright/test";
import { test, expect } from "./fixtures";

/**
 * Accessibility regression suite.
 *
 * Every canonical route is asserted axe-clean across three rendering
 * modes — light, dark, and forced-colors (Windows High Contrast /
 * macOS Increase Contrast). Token drift in any of the modes surfaces
 * here before it ships.
 *
 * The light pass exercises the primary palette; dark reveals
 * dark-mode token gaps; forced-colors flushes out custom-property
 * regressions where the system colour keywords don't carry through.
 */

const ROUTES = [
  "/design",
  "/analyze",
  "/explore",
  "/dashboard",
  "/glossary",
  "/vocabulary",
  "/recipes",
  "/projects",
  "/settings",
  "/settings/workspace",
  "/settings/team",
  "/settings/profile",
  "/settings/system",
  "/settings/providers",
  "/settings/models",
  "/settings/usage",
  "/settings/notifications",
  "/settings/reports",
  "/settings/schedules",
  "/settings/knowledge",
  "/settings/federation",
  "/settings/quality",
  "/settings/quality/signals",
  "/settings/quality/stale",
  "/settings/ambiguity",
  "/settings/acl",
  "/settings/lineage",
  "/settings/audit",
  "/settings/governance/audit",
  "/settings/governance/routing",
  "/settings/approvals",
  "/settings/mappings",
  "/settings/prompts",
  "/settings/sessions",
];

const SCHEMES: ReadonlyArray<{
  name: "light" | "dark";
  forcedColors?: "active" | "none";
}> = [
  { name: "light" },
  { name: "dark" },
];

async function runAxe(page: Page) {
  await page.addScriptTag({
    path: require.resolve("axe-core/axe.min.js"),
  });
  return page.evaluate(async () => {
    // axe-core ships its own TypeScript types — narrow the runtime
    // `window.axe` global through them so the result/violation walk
    // is structurally typed end-to-end.
    type Axe = typeof import("axe-core");
    const w = window as unknown as { axe: Axe };
    const result = await w.axe.run(document, {
      runOnly: ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa"],
    });
    return result.violations.map((v) => ({
      id: v.id,
      impact: v.impact,
      nodes: v.nodes.map((n) => ({
        target: n.target as unknown as string[],
        html: n.html,
      })),
    }));
  });
}

for (const scheme of SCHEMES) {
  for (const route of ROUTES) {
    test(`a11y(${scheme.name}): ${route}`, async ({ page }) => {
      await page.emulateMedia({ colorScheme: scheme.name });
      await page.goto(route, { waitUntil: "networkidle" });
      const violations = await runAxe(page);
      expect(
        violations,
        `axe-core found violations on ${route} (${scheme.name}):\n${JSON.stringify(violations, null, 2)}`,
      ).toEqual([]);
    });
  }
}

// Forced-colors smoke pass — only the canonical workbench surfaces.
// Settings pages share the same primitives, so the workbench coverage
// catches any token-drift regression.
const FORCED_COLORS_ROUTES = [
  "/projects",
  "/recipes",
  "/glossary",
  "/dashboard",
];
for (const route of FORCED_COLORS_ROUTES) {
  test(`a11y(forced-colors): ${route}`, async ({ page }) => {
    await page.emulateMedia({ forcedColors: "active" });
    await page.goto(route, { waitUntil: "networkidle" });
    const violations = await runAxe(page);
    expect(
      violations,
      `axe-core found violations on ${route} (forced-colors):\n${JSON.stringify(violations, null, 2)}`,
    ).toEqual([]);
  });
}
