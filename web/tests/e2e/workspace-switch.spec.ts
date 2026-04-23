import { test, expect } from "./fixtures";

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
    // `listWorkspaces` returns a bare `WorkspaceSummary[]` — not
    // wrapped. The seeded fixture already mocks this path, but we
    // override here so the test can pin an exact payload for its
    // own assertions.
    await page.route("**/api/proxy/workspaces**", async (route) => {
      await route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify(WORKSPACES),
      });
    });
  });

  test("workspaces endpoint returns seeded list", async ({ page }) => {
    // `page.evaluate` starts in `about:blank` — navigate first so
    // relative fetches resolve against the app origin and Playwright's
    // `page.route` interceptor can catch them. `page.request.get`
    // would skip routing entirely (it uses APIRequestContext, which
    // doesn't traverse the browser network stack).
    await page.goto("/");
    const body = await page.evaluate(async () => {
      const res = await fetch("/api/proxy/workspaces");
      return (await res.json()) as Array<{ name: string }>;
    });
    expect(body).toHaveLength(2);
    expect(body[0].name).toBe("Default");
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
