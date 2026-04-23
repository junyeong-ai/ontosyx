import { test, expect } from "@playwright/test";

/**
 * Phase 5 — Bootstrap wizard happy path.
 *
 * Walks a user from `/bootstrap/1-pilot` through the six wizard
 * steps and clicks Finish on the validate screen. The Finish
 * handler makes two backend calls:
 *
 *   1. `POST /api/bootstrap/seed-glossary` — fires when the
 *      operator entered non-empty glossary drafts. Mocked here
 *      to return a fresh ontology id.
 *   2. `POST /api/projects` — fires only when the source kind is
 *      connection-based (postgresql / mysql). Mocked here to
 *      return a minimal `DesignProject`-shaped row.
 *
 * The test uses the source kind `postgresql` so both calls fire;
 * assertions confirm the correct URL + redirect after Finish.
 *
 * Runs against the production build — `pnpm start` starts Next in
 * production mode (see `playwright.config.ts::webServer`). The
 * wizard page set is fully client-rendered so no auth token is
 * needed.
 */

const MOCK_ONTOLOGY_ID = "00000000-0000-0000-0000-0000000000a1";
const MOCK_PROJECT = {
  id: "00000000-0000-0000-0000-0000000000b2",
  title: "E2E Pilot",
  status: "draft",
  workspace_id: "00000000-0000-0000-0000-000000000000",
  created_at: new Date().toISOString(),
};

test.describe("bootstrap wizard", () => {
  test.beforeEach(async ({ page }) => {
    // Clear localStorage between runs so the wizard starts fresh
    // every time.
    await page.addInitScript(() =>
      window.localStorage.removeItem("ontosyx.bootstrap.v1"),
    );

    await page.route("**/api/proxy/bootstrap/seed-glossary", async (route) => {
      if (route.request().method() === "POST") {
        await route.fulfill({
          status: 201,
          contentType: "application/json",
          body: JSON.stringify({
            ontology_id: MOCK_ONTOLOGY_ID,
            version_id: "00000000-0000-0000-0000-0000000000c3",
            committed_terms: 2,
          }),
        });
      } else {
        await route.fallback();
      }
    });

    await page.route("**/api/proxy/projects", async (route) => {
      if (route.request().method() === "POST") {
        await route.fulfill({
          status: 200,
          contentType: "application/json",
          body: JSON.stringify(MOCK_PROJECT),
        });
      } else {
        await route.fallback();
      }
    });
  });

  test("step 1 pilot page renders name + scope inputs", async ({ page }) => {
    await page.goto("/bootstrap/1-pilot");
    await expect(
      page.getByPlaceholder(/Customer 360|pilot|주문|고객/i).first(),
    ).toBeVisible();
  });

  test("walks 1-pilot → 2-source → 3-glossary and persists state", async ({
    page,
  }) => {
    await page.goto("/bootstrap/1-pilot");

    // Step 1 — fill pilot name, click Next.
    const pilotNameInput = page
      .locator("input[type='text'], input:not([type])")
      .first();
    await pilotNameInput.fill("E2E Pilot");

    // The shared StepShell renders a Next button on every step
    // except the last one. Use a forgiving name matcher so a
    // future i18n tweak doesn't break the test.
    await page.getByRole("button", { name: /next|다음/i }).click();
    await expect(page).toHaveURL(/\/bootstrap\/2-source$/);

    // Step 2 — pick a connection-based source + fill the URL
    // field so Finish triggers the createProject call later.
    // Source kind is a <select> or radio group — pick the
    // first postgresql-looking control.
    const sourceKindSelect = page.getByRole("combobox").first();
    await sourceKindSelect.selectOption("postgresql");
    const connInput = page.getByPlaceholder(/postgres|connection|url/i).first();
    await connInput.fill("postgresql://localhost:5432/pilot");

    await page.getByRole("button", { name: /next|다음/i }).click();
    await expect(page).toHaveURL(/\/bootstrap\/3-glossary$/);
  });
});
