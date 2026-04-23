import { test, expect } from "@playwright/test";

/**
 * Phase 5 — Ambiguity resolution end-to-end.
 *
 * Loads /settings/ambiguity with one pending ambiguity context, then
 * clicks Resolve to open the resolution modal. Asserts the modal
 * surfaces the ambiguous column (relation + column) and the
 * clarification prompt so the operator knows what choice they are
 * about to make.
 *
 * Mocked hops:
 *   - `GET /api/proxy/ambiguity-contexts` — list with a single pending
 *     entry.
 */

const PENDING_ENTRY = {
  context: {
    id: "ctx-pending-1",
    source_id: "src-postgres",
    column: { relation: "orders", column: "status" },
    kind: { kind: "numeric_code" },
    sample_values: ["1", "2", "3"],
    clarification_prompt:
      "Which code system catalogs 1/2/3 as order statuses?",
    detection_source_hash: "sha256:abc123",
    detected_at: "2026-04-22T00:00:00Z",
  },
  active_resolution: null,
};

test.describe("ambiguity resolution", () => {
  test.beforeEach(async ({ page }) => {
    await page.route(
      /\/api\/proxy\/ambiguity-contexts(\?.*)?$/,
      async (route) => {
        await route.fulfill({
          status: 200,
          contentType: "application/json",
          body: JSON.stringify({ items: [PENDING_ENTRY] }),
        });
      },
    );
  });

  test("clicking Resolve surfaces the clarification prompt in the modal", async ({
    page,
  }) => {
    await page.goto("/settings/ambiguity");
    await page.waitForLoadState("domcontentloaded");

    // Pending tab is the default and the single row renders
    // `orders.status`. Match loosely because the row composes
    // relation + column with an interpunct.
    await expect(page.getByText(/orders/).first()).toBeVisible();

    // Trigger the resolution modal.
    await page
      .getByRole("button", { name: /^Resolve$/ })
      .first()
      .click();

    // The modal heading interpolates relation + column.
    await expect(
      page.getByText(/Resolve orders\.status/i),
    ).toBeVisible();
    // The clarification prompt from the detection hash is shown so
    // the operator picks the right code system / glossary term.
    await expect(
      page.getByText(
        /Which code system catalogs 1\/2\/3 as order statuses/,
      ),
    ).toBeVisible();
  });
});
