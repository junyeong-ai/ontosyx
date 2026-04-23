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

const EDIT_RECEIPT = {
  ontology_id: ONT_ID,
  base_version: 3,
  commit_version: 4,
  commit_version_id: "ver-4",
  change_type: "bind_property_to_term",
  applied_operations: 1,
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
    await page.route(
      /\/api\/proxy\/ontologies\/.*\/edits$/,
      async (route) => {
        if (route.request().method() === "POST") {
          await route.fulfill({
            status: 201,
            contentType: "application/json",
            body: JSON.stringify(EDIT_RECEIPT),
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

  test("batch bind posts BindPropertyToTerm ops to /edits with expected_version", async ({
    page,
  }) => {
    await page.goto("/settings/glossary/bindings");
    await page.waitForLoadState("domcontentloaded");

    // Fill term + termId (both required for the batch submit to fire).
    const inputs = page.locator("input[type='text']");
    await inputs.first().fill("customer contact");
    // `termId` is the second input (see BindingPanel fields order).
    await inputs.nth(1).fill("glossary-customer-contact");

    // Trigger the scorer so candidates render + select the first.
    await page
      .getByRole("button", { name: /suggest|bind|검색/i })
      .first()
      .click();
    await expect(page.getByText(/email/).first()).toBeVisible();

    // Check the first candidate's checkbox — this unlocks the
    // `Apply` button that fires the /edits POST.
    const firstCheckbox = page.getByRole("checkbox").first();
    await firstCheckbox.check();

    // Capture the POST so we can assert the payload shape matches
    // the backend `OntologyEditRequest` contract.
    const editRequest = page.waitForRequest(
      (req) =>
        /\/api\/proxy\/ontologies\/.*\/edits$/.test(req.url()) &&
        req.method() === "POST",
    );
    // The apply button sits below the candidates list — find it by
    // any label that matches the common "apply"/"bind selected" idiom.
    await page
      .getByRole("button", { name: /apply|commit|적용|바인딩/i })
      .last()
      .click();
    const req = await editRequest;

    const body = req.postDataJSON() as {
      expected_version: number;
      operations: Array<{
        op: string;
        owner: { kind: string; type_id: string };
        property_id: string;
        glossary_term_id: string;
      }>;
      message: string;
    };
    expect(body.expected_version).toBe(3);
    expect(body.operations).toHaveLength(1);
    expect(body.operations[0].op).toBe("bind_property_to_term");
    expect(body.operations[0].property_id).toBe("prop-email");
    expect(body.operations[0].glossary_term_id).toBe(
      "glossary-customer-contact",
    );
  });
});
