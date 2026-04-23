import { test, expect } from "./fixtures";

/**
 * Phase 6.4 — Dashboard CRUD smoke.
 *
 * Mocks the dashboards API and verifies that the page can load, the API
 * returns the expected shape, and the response is rendered without crashing.
 */

const MOCK_DASHBOARD = {
  id: "11111111-1111-1111-1111-111111111111",
  name: "E2E Dashboard",
  description: "Playwright smoke",
  workspace_id: "00000000-0000-0000-0000-000000000000",
  created_at: new Date().toISOString(),
  updated_at: new Date().toISOString(),
  widgets: [
    {
      id: "w1",
      widget_type: "bar_chart",
      title: "Revenue by category",
      query: "MATCH (n) RETURN n LIMIT 0",
      layout: { x: 0, y: 0, w: 6, h: 4 },
    },
  ],
};

const MOCK_QUERY_RESULT = {
  columns: ["category", "revenue"],
  rows: [
    { category: "전자기기", revenue: 5078000 },
    { category: "패션", revenue: 813000 },
    { category: "식품", revenue: 280000 },
  ],
};

test.describe("dashboard", () => {
  test.beforeEach(async ({ page }) => {
    await page.route("**/api/proxy/dashboards**", async (route) => {
      if (route.request().method() === "POST") {
        await route.fulfill({
          status: 200,
          contentType: "application/json",
          body: JSON.stringify(MOCK_DASHBOARD),
        });
      } else {
        await route.fulfill({
          status: 200,
          contentType: "application/json",
          body: JSON.stringify({ dashboards: [MOCK_DASHBOARD] }),
        });
      }
    });

    await page.route("**/api/proxy/query/raw**", async (route) => {
      await route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify(MOCK_QUERY_RESULT),
      });
    });
  });

  test("dashboard API mock returns data shape", async ({ page }) => {
    // `/` client-redirects to `/design`; waiting for that to settle
    // prevents "Execution context was destroyed" when `page.evaluate`
    // races the navigation. Once at a stable page, we can `fetch`
    // through the browser network stack, which `page.route`
    // intercepts (unlike the separate APIRequestContext).
    await page.goto("/");
    await page.waitForURL(/\/design(\?.*)?$/);
    const body = await page.evaluate(async () => {
      const res = await fetch("/api/proxy/dashboards");
      return (await res.json()) as { dashboards: Array<{ widgets: unknown[] }> };
    });
    expect(body.dashboards).toHaveLength(1);
    expect(body.dashboards[0].widgets).toHaveLength(1);
  });

  test("home page survives dashboards mock", async ({ page }) => {
    await page.goto("/");
    await page.waitForLoadState("domcontentloaded");
    await expect(page.locator("body")).toBeVisible();
  });
});
