import { test, expect } from "./fixtures";
import {
  mockOntologyDetail,
  mockOntologyIR,
  mockQueryDiagnostic,
  mockQueryMetadata,
  mockQueryProvenance,
  mockQueryResult,
} from "./mocks";

/**
 * Phase 5 — Query → ResponseBasis end-to-end.
 *
 * Loads the Analyze workspace, switches the right pane to the Query
 * tab, runs a raw Cypher snippet, and asserts the ResponseBasis
 * panel surfaces the provenance fields the backend stamps onto
 * `QueryResult.metadata`.
 *
 * Both backend hops are mocked:
 *   - `POST /api/proxy/query/raw` — returns rows + metadata
 *     (provenance + warnings).
 *   - `GET  /api/proxy/ontologies/<id>` — used by ResponseBasis to
 *     resolve `type_ids` to human labels with description tooltips.
 */

const ONT_ID = "00000000-0000-0000-0000-0000000010ff";

// `rawQuery` in `src/lib/api/queries.ts` unwraps `raw.results`, so
// the mock body wraps the QueryResult in `{ query, target, results }`
// — matching what the backend returns today.
const QUERY_RESULT = {
  query: "MATCH (n) RETURN n LIMIT 1",
  target: "graph",
  results: mockQueryResult({
    rows: [{ n: { id: 1, labels: ["Customer"], properties: { tier: "gold" } } }],
    metadata: mockQueryMetadata({
      provenance: mockQueryProvenance({ ontology_id: ONT_ID }),
      warnings: [mockQueryDiagnostic()],
    }),
  }),
};

// `resolveTypeIds` inside ResponseBasis reads
// `detail.ontology_ir.node_types[].{id,label}` to map
// `type-customer` → "Customer".
const ONTOLOGY_DETAIL = mockOntologyDetail({
  id: ONT_ID,
  ontology_ir: mockOntologyIR({
    node_types: [
      {
        id: "type-customer",
        label: "Customer",
        description: "End-user buyer",
        properties: [],
      },
    ],
  }),
});

test.describe("query → response basis", () => {
  test.beforeEach(async ({ page }) => {
    await page.route("**/api/proxy/query/raw", async (route) => {
      if (route.request().method() === "POST") {
        await route.fulfill({
          status: 200,
          contentType: "application/json",
          body: JSON.stringify(QUERY_RESULT),
        });
      } else {
        await route.fallback();
      }
    });

    await page.route(
      new RegExp(`/api/proxy/ontologies/${ONT_ID}(\\?.*)?$`),
      async (route) => {
        await route.fulfill({
          status: 200,
          contentType: "application/json",
          body: JSON.stringify(ONTOLOGY_DETAIL),
        });
      },
    );
  });

  test("running a raw query renders rows + ResponseBasis with provenance + warning", async ({
    page,
  }) => {
    await page.goto("/analyze");
    await page.waitForLoadState("domcontentloaded");

    // Switch the right pane to the Query tab — the TabBar uses
    // Base UI `Tabs.Tab` so the accessibility role is "tab".
    await page.getByRole("tab", { name: /^Query$/ }).click();

    // QueryPanel renders a plain `<textarea>` with the canonical
    // "MATCH (n) RETURN n LIMIT 10" placeholder. Targeting by
    // placeholder skips the disabled chat input at the bottom of
    // the analyze shell, which also uses a `<textarea>`.
    await page
      .getByPlaceholder(/^MATCH \(n\) RETURN n LIMIT/i)
      .fill("MATCH (n) RETURN n LIMIT 1");

    const queryRequest = page.waitForRequest(
      (req) =>
        req.url().includes("/api/proxy/query/raw") &&
        req.method() === "POST",
    );
    await page.getByRole("button", { name: /^Execute$/ }).click();
    await queryRequest;

    // `<section aria-label="Response basis">` — picked up as a
    // `region` by Playwright's accessibility tree.
    const basis = page.getByRole("region", { name: /^Response basis$/ });
    await expect(basis).toBeVisible();

    // Provenance rows: version + filter render verbatim, type_ids
    // resolve through the mocked ontology detail to "Customer".
    await expect(basis.getByText("v3")).toBeVisible();
    await expect(basis.getByText("n.active = true")).toBeVisible();
    await expect(basis.getByText("Customer").first()).toBeVisible();
    await expect(basis.getByText("src-postgres")).toBeVisible();

    // Advisory warnings render beneath the provenance grid.
    await expect(
      basis.getByText(/unbounded variable-length pattern/),
    ).toBeVisible();
  });
});
