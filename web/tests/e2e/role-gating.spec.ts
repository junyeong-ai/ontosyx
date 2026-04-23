import { test, expect } from "./fixtures";

/**
 * Phase 5+ — Role-gated UI coverage.
 *
 * The federation settings page is explicitly admin-only: its
 * `useAuth().isAdmin` gate short-circuits the render to a
 * "admin privileges required" notice. We exercise both the admin
 * happy path (via the default fixture) and the viewer denial path
 * (via `test.use({ role: "viewer" })`) so that the gate can't
 * regress silently — a future refactor that leaks the adapter form
 * to non-admins would fail this spec immediately.
 *
 * We stub `/admin/federation/adapters` for both runs because the
 * admin render still fetches it; the viewer render never does.
 */

test.describe("federation settings — role gate", () => {
  test.beforeEach(async ({ page }) => {
    await page.route(
      /\/api\/proxy\/admin\/federation\/adapters(\?.*)?$/,
      async (route) => {
        await route.fulfill({
          status: 200,
          contentType: "application/json",
          body: JSON.stringify([]),
        });
      },
    );
    await page.route(
      /\/api\/proxy\/admin\/federation\/health(\?.*)?$/,
      async (route) => {
        await route.fulfill({
          status: 200,
          contentType: "application/json",
          body: JSON.stringify({
            workspace_id: "00000000-0000-0000-0000-000000000000",
            resolver_hydrated: true,
            resolver_count: 0,
            store_count: 0,
            in_sync: true,
            orphans_in_resolver: [],
            missing_from_resolver: [],
          }),
        });
      },
    );
  });

  test("admin sees the register-adapter form", async ({ page }) => {
    await page.goto("/settings/federation");
    await page.waitForLoadState("domcontentloaded");
    // The "Register adapter" section heading exists in the admin
    // branch. Find it by role+name so future styling changes don't
    // rot the selector.
    await expect(
      page.getByRole("heading", { name: /^Register adapter$/ }),
    ).toBeVisible();
  });

  test.describe("as viewer", () => {
    test.use({ role: "viewer" });

    test("viewer hits the admin-only gate and never sees the form", async ({
      page,
    }) => {
      await page.goto("/settings/federation");
      await page.waitForLoadState("domcontentloaded");
      await expect(
        page.getByText(
          /Federation adapter management requires admin privileges/,
        ),
      ).toBeVisible();
      // The register form must not render in the viewer branch — if
      // someone ever lifts the gate accidentally, this assertion
      // fails.
      await expect(
        page.getByRole("heading", { name: /^Register adapter$/ }),
      ).toHaveCount(0);
    });
  });
});
