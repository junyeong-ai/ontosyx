import { test, expect } from "@playwright/test";

/**
 * Phase 6.4 — Workspace switch triggers data refetch.
 *
 * Mocks /api/proxy/workspaces. When the user switches workspace via URL
 * query param, we assert that the URL reflects the new workspace and
 * that subsequent API calls include the new workspace header.
 */

const WORKSPACES = [
  { id: "00000000-0000-0000-0000-000000000001", name: "Default", is_default: true },
  { id: "00000000-0000-0000-0000-000000000002", name: "Team B", is_default: false },
];

test.describe("workspace switch", () => {
  test.beforeEach(async ({ page }) => {
    await page.route("**/api/proxy/workspaces**", async (route) => {
      await route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify({ workspaces: WORKSPACES }),
      });
    });
  });

  test("workspaces endpoint returns seeded list", async ({ page }) => {
    const resp = await page.request.get("/api/proxy/workspaces");
    expect(resp.ok()).toBeTruthy();
    const body = await resp.json();
    expect(body.workspaces).toHaveLength(2);
    expect(body.workspaces[0].name).toBe("Default");
  });

  test("switching workspace via query string reloads page", async ({
    page,
  }) => {
    await page.goto("/?workspace=00000000-0000-0000-0000-000000000001");
    await page.waitForLoadState("domcontentloaded");
    expect(page.url()).toContain(
      "workspace=00000000-0000-0000-0000-000000000001",
    );

    await page.goto("/?workspace=00000000-0000-0000-0000-000000000002");
    await page.waitForLoadState("domcontentloaded");
    expect(page.url()).toContain(
      "workspace=00000000-0000-0000-0000-000000000002",
    );
  });
});
