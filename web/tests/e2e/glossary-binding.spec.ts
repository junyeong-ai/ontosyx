import { test, expect } from "@playwright/test";

/**
 * Phase 5 — Glossary binding → rule suggestion end-to-end.
 *
 * Loads the /settings/glossary/bindings page with a mocked
 * ontology list, asserts that the binding panel mounts for the
 * first ontology, and exercises the `suggest` mutation — the scorer
 * returns two property candidates that must both appear in the UI
 * so the designer can tick the ones they want to batch-commit.
 *
 * Mocked hops:
 *   - `GET  /api/proxy/ontologies?limit=1` — current ontology
 *   - `POST /api/proxy/ontologies/{id}/binding-suggestions/suggest`
 *     — scorer output with two candidates
 */

const ONT_ID = "00000000-0000-0000-0000-0000000030bb";

const LIST_RESPONSE = {
  items: [
    {
      id: ONT_ID,
      lineage_id: "lin-pilot",
      name: "Pilot",
      description: { default: "E2E glossary pilot" },
      created_at: "2026-04-22T00:00:00Z",
      updated_at: "2026-04-22T00:00:00Z",
      current_version: { version: 3, version_id: "ver-3" },
    },
  ],
  next_cursor: null,
};

const SUGGEST_RESPONSE = {
  candidates: [
    {
      owner_kind: "node",
      owner_type_id: "type-customer",
      owner_label: "Customer",
      property_id: "prop-email",
      property_name: "email",
      score: 0.92,
      reasons: ["Name match: customer_email"],
    },
    {
      owner_kind: "node",
      owner_type_id: "type-customer",
      owner_label: "Customer",
      property_id: "prop-phone",
      property_name: "phone",
      score: 0.68,
      reasons: ["Alias match: contact_number"],
    },
  ],
};

test.describe("glossary binding", () => {
  test.beforeEach(async ({ page }) => {
    await page.route(
      /\/api\/proxy\/ontologies(\?.*)?$/,
      async (route) => {
        if (route.request().method() === "GET") {
          await route.fulfill({
            status: 200,
            contentType: "application/json",
            body: JSON.stringify(LIST_RESPONSE),
          });
        } else {
          await route.fallback();
        }
      },
    );
    await page.route(
      /\/api\/proxy\/ontologies\/.*\/binding-suggestions\/suggest$/,
      async (route) => {
        if (route.request().method() === "POST") {
          await route.fulfill({
            status: 200,
            contentType: "application/json",
            body: JSON.stringify(SUGGEST_RESPONSE),
          });
        } else {
          await route.fallback();
        }
      },
    );
  });

  test("submitting a term renders both scored property candidates", async ({
    page,
  }) => {
    await page.goto("/settings/glossary/bindings");
    await page.waitForLoadState("domcontentloaded");

    // The panel mounts once the ontology list resolves. The first
    // text input in the panel is the term field.
    const termInput = page.locator("input[type='text']").first();
    await termInput.fill("customer contact");

    const suggestRequest = page.waitForRequest(
      (req) =>
        /\/binding-suggestions\/suggest$/.test(req.url()) &&
        req.method() === "POST",
    );
    // The scorer trigger is the first `button` in the panel body.
    await page.getByRole("button", { name: /suggest|bind|검색/i }).first().click();
    await suggestRequest;

    // Both candidate property names render.
    await expect(page.getByText(/email/).first()).toBeVisible();
    await expect(page.getByText(/phone/).first()).toBeVisible();
    // Score bands — top row shows a >=0.9 band; second row a 0.6-0.8 band.
    await expect(page.getByText(/0\.92|92%/).first()).toBeVisible();
  });
});
