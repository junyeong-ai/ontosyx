import { test, expect } from "./fixtures";
import {
  mockOntologyEditReceipt,
  mockOntologyListItem,
  mockPropertyCandidate,
  mockSuggestBindingsResponse,
} from "./mocks";

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
 *
 * Factory-backed mocks (`./mocks`) — the schema drift that bricked
 * this spec last month (missing `PropertyCandidate.signals`) can't
 * happen now; the factory fails to compile if the wire type adds a
 * required field without a default.
 */

const ONT_ID = "00000000-0000-0000-0000-0000000030bb";

const LIST_RESPONSE = {
  items: [
    mockOntologyListItem({
      id: ONT_ID,
      lineage_id: "lin-pilot",
      name: "Pilot",
      description: { default: "E2E glossary pilot" },
    }),
  ],
  next_cursor: null,
};

const SUGGEST_RESPONSE = mockSuggestBindingsResponse({
  ontology_id: ONT_ID,
  candidates: [
    mockPropertyCandidate({
      property_id: "prop-email",
      property_name: "email",
      score: 0.92,
      signals: [{ kind: "canonical_name" }],
    }),
    mockPropertyCandidate({
      property_id: "prop-phone",
      property_name: "phone",
      score: 0.68,
      signals: [{ kind: "alias", detail: "contact_number" }],
    }),
  ],
});

const EDIT_RECEIPT = mockOntologyEditReceipt({
  new_version: 4,
  applied_operations: 1,
});

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
      /\/api\/proxy\/ontologies\/[^/]+\/glossary\/suggest-bindings$/,
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

    // Required-marker suffix (`*`) is part of the label's text
    // content, so `getByLabel` sees "Term*". Anchoring with `^$`
    // keeps the match from also hitting "Term id".
    await page
      .getByLabel(/^Term\*?$/)
      .fill("customer contact");

    const suggestRequest = page.waitForRequest(
      (req) =>
        /\/glossary\/suggest-bindings$/.test(req.url()) &&
        req.method() === "POST",
    );
    await page
      .getByRole("button", { name: /^Score candidates$/ })
      .click();
    await suggestRequest;

    // Both candidate property names render inside the candidates table.
    await expect(page.getByText("email").first()).toBeVisible();
    await expect(page.getByText("phone").first()).toBeVisible();
  });

  test("batch bind posts BindPropertyToTerm ops to /edits with expected_version", async ({
    page,
  }) => {
    await page.goto("/settings/glossary/bindings");
    await page.waitForLoadState("domcontentloaded");

    // Term + termId are both required for the batch submit to fire
    // — the panel refuses to POST /edits when termId is blank.
    await page
      .getByLabel(/^Term\*?$/)
      .fill("customer contact");
    await page
      .getByLabel("Term id")
      .fill("glossary-customer-contact");

    await page
      .getByRole("button", { name: /^Score candidates$/ })
      .click();
    await expect(page.getByText("email").first()).toBeVisible();

    // Check the first candidate's checkbox — selects one op for the
    // batch commit. The apply button only renders once at least one
    // checkbox is ticked.
    await page.getByRole("checkbox").first().check();

    const editRequest = page.waitForRequest(
      (req) =>
        /\/api\/proxy\/ontologies\/.*\/edits$/.test(req.url()) &&
        req.method() === "POST",
    );
    // i18n renders the commit button as "Bind {n} selected".
    await page
      .getByRole("button", { name: /^Bind \d+ selected$/ })
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
