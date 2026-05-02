import { test, expect, type Page } from "@playwright/test";

/**
 * Accessibility regression suite.
 *
 * For every canonical route, run axe-core in the browser and assert
 * zero violations across all enabled rules. The pages here are the
 * ones the audit covered + the ones a contributor is likely to land
 * a regression on.
 *
 * Setup: axe-core is loaded via `@axe-core/react` in development; in
 * Playwright we inject the standalone `axe.min.js` from the package
 * resolved via dev dependency.
 */

const ROUTES = [
  "/recipes",
  "/vocabulary",
  "/glossary",
  "/dashboard",
  "/design",
  "/analyze",
  "/explore",
  "/projects",
  "/settings/quality",
  "/settings/governance/routing",
  "/settings/governance/audit",
  "/settings/notifications",
  "/settings/models",
  "/settings/audit",
];

async function runAxe(page: Page) {
  await page.addScriptTag({
    path: require.resolve("axe-core/axe.min.js"),
  });
  return page.evaluate(async () => {
    // axe loaded onto window by the script tag above.
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const w = window as unknown as { axe: any };
    const result = await w.axe.run(document, {
      runOnly: ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa"],
    });
    return result.violations.map((v: { id: string; impact: string; nodes: { target: string[]; html: string }[] }) => ({
      id: v.id,
      impact: v.impact,
      nodes: v.nodes.map((n) => ({ target: n.target, html: n.html })),
    }));
  });
}

for (const route of ROUTES) {
  test(`a11y: ${route}`, async ({ page }) => {
    await page.goto(route, { waitUntil: "networkidle" });
    const violations = await runAxe(page);
    expect(
      violations,
      `axe-core found violations on ${route}:\n${JSON.stringify(violations, null, 2)}`,
    ).toEqual([]);
  });
}
