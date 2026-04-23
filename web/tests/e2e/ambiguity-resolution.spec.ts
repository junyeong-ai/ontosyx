import { test, expect } from "./fixtures";

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

const RESOLVE_RESPONSE = {
  id: "res-1",
  context_id: "ctx-pending-1",
  context_source_hash: "sha256:abc123",
  mapping: {
    kind: "glossary_ref",
    term_id: "glossary-order-status",
  },
  resolved_at: "2026-04-23T00:00:00Z",
};

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
    // the operator picks the right code system / glossary term.
    await expect(
      page.getByText(
        /Which code system catalogs 1\/2\/3 as order statuses/,
      ),
    ).toBeVisible();
  });

  // FIXME: clicking Resolve opens the modal, but the subsequent
  // mode-switch + term-id fill + submit sequence doesn't persist
  // — needs the resolution-modal test double refactored so the
  // glossary_ref mode is addressable by a stable role/name pair
  // rather than the current "pick whichever button matches the
  // regex union" heuristic.
  test.fixme(
    "submitting a glossary_ref posts {mapping: {kind, term_id}} to /resolve",
    async ({ page }) => {
    await page.goto("/settings/ambiguity");
    await page.waitForLoadState("domcontentloaded");
    await page.getByRole("button", { name: /^Resolve$/ }).first().click();
    await expect(
      page.getByText(/Resolve orders\.status/i),
    ).toBeVisible();

    // Switch the mode to glossary_ref and fill the term id.
    // The modal has a radio or tab group for the 3 modes — match by
    // label and pick Glossary.
    const glossaryTab = page
      .getByRole("button", { name: /glossary|용어/i })
      .first();
    if (await glossaryTab.isVisible()) {
      await glossaryTab.click();
    }
    // The term id input lives under the glossary mode — it's a
    // regular `<input>`. Try the first input whose placeholder
    // matches the hint; fall back to any input in the modal.
    const termInput = page
      .locator("input[type='text']")
      .first();
    await termInput.fill("glossary-order-status");

    // Wait on the POST so we can assert the payload.
    const resolveRequest = page.waitForRequest(
      (req) =>
        /\/api\/proxy\/ambiguities\/[^/]+\/resolve$/.test(req.url()) &&
        req.method() === "POST",
    );
    await page
      .getByRole("button", { name: /save|submit|apply|저장|확인/i })
      .last()
      .click();
    const req = await resolveRequest;

    const body = req.postDataJSON() as {
      mapping: { kind: string; term_id?: string; code_system_id?: string };
    };
    expect(body.mapping.kind).toBe("glossary_ref");
    expect(body.mapping.term_id).toBe("glossary-order-status");
    // The resolve URL encodes the context id from the list row.
    expect(req.url()).toMatch(/ambiguities\/ctx-pending-1\/resolve$/);
    },
  );
});
