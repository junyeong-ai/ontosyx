import { test, expect } from "./fixtures";
import {
  mockAmbiguityContext,
  mockAmbiguityResolution,
  mockAmbiguitySummary,
} from "./mocks";

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
 *   - `GET /api/proxy/ambiguities` — list with a single pending entry.
 *   - `POST /api/proxy/ambiguities/:id/resolve` — resolution commit.
 */

const PENDING_ENTRY = mockAmbiguitySummary({
  context: mockAmbiguityContext({
    id: "ctx-pending-1",
    clarification_prompt:
      "Which code system catalogs 1/2/3 as order statuses?",
    detection_source_hash: "sha256:abc123",
  }),
});

const RESOLVE_RESPONSE = mockAmbiguityResolution({
  context_id: "ctx-pending-1",
  context_source_hash: "sha256:abc123",
  mapping: { kind: "concept_ref", concept_id: "c-order-status" },
});

test.describe("ambiguity resolution", () => {
  test.beforeEach(async ({ page }) => {
    await page.route(
      /\/api\/proxy\/ambiguities(\?.*)?$/,
      async (route) => {
        await route.fulfill({
          status: 200,
          contentType: "application/json",
          body: JSON.stringify({ items: [PENDING_ENTRY] }),
        });
      },
    );
    await page.route(
      /\/api\/proxy\/ambiguities\/[^/]+\/resolve$/,
      async (route) => {
        if (route.request().method() === "POST") {
          await route.fulfill({
            status: 200,
            contentType: "application/json",
            body: JSON.stringify(RESOLVE_RESPONSE),
          });
        } else {
          await route.fallback();
        }
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
    // the operator picks the right code system / concept.
    await expect(
      page.getByText(
        /Which code system catalogs 1\/2\/3 as order statuses/,
      ),
    ).toBeVisible();
  });

  test("submitting a concept_ref posts {mapping: {kind, concept_id}} to /resolve", async ({
    page,
  }) => {
    await page.goto("/settings/ambiguity");
    await page.waitForLoadState("domcontentloaded");
    await page.getByRole("button", { name: /^Resolve$/ }).first().click();
    await expect(
      page.getByText(/Resolve orders\.status/i),
    ).toBeVisible();

    // Mode switcher is a radio group. The `<input type="radio">` is
    // `.sr-only` wrapped by a `<label>` carrying the translated
    // "Concept" copy — `.check({ force: true })` is required
    // because the input stays visually hidden.
    await page
      .getByRole("radio", { name: /^Concept$/ })
      .check({ force: true });

    await page.getByLabel(/^Concept id$/).fill("c-order-status");

    const resolveRequest = page.waitForRequest(
      (req) =>
        /\/api\/proxy\/ambiguities\/[^/]+\/resolve$/.test(req.url()) &&
        req.method() === "POST",
    );
    // Footer's Save button — "Save resolution" in en.
    await page.getByRole("button", { name: /^Save resolution$/ }).click();
    const req = await resolveRequest;

    const body = req.postDataJSON() as {
      mapping: { kind: string; concept_id?: string; code_system_id?: string };
    };
    expect(body.mapping.kind).toBe("concept_ref");
    expect(body.mapping.concept_id).toBe("c-order-status");
    // The resolve URL encodes the context id from the list row.
    expect(req.url()).toMatch(/ambiguities\/ctx-pending-1\/resolve$/);
  });
});
