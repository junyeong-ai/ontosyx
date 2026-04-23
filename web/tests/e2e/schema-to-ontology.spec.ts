import { test, expect } from "./fixtures";

/**
 * Phase 5 — Schema → ontology proposal.
 *
 * Opens the Complete Map view for a mocked ontology. The backend returns
 * a map-summary populated from a schema introspection (topology +
 * vocabulary axes carry counts that the map surface visualises), and a
 * dangling-references callout exercises the Phase 1.7 integrity check
 * that gates downstream proposal acceptance.
 *
 * Mocked hops:
 *   - `GET /api/proxy/ontologies/{id}/map-summary` — axis entries +
 *     danglers
 *   - `GET /api/proxy/ontologies/{id}/cross-refs` — empty so the flow
 *     renders without crashing
 */

const ONT_ID = "00000000-0000-0000-0000-0000000020aa";

// Wire-shape note: the backend emits `kind` tokens in snake_case
// (`node_types`, `edge_types`, `glossary_terms`, …) — the i18n
// bundle for `ontology.map.axes.*.kinds.*` is keyed on the same
// tokens. PascalCase here would make next-intl fall through to raw
// keys in axis labels, so stick to the canonical enum.
const MAP_SUMMARY = {
  ontology_id: ONT_ID,
  version: "v2",
  topology: {
    entries: [
      { kind: "node_types", count: 3 },
      { kind: "edge_types", count: 2 },
    ],
  },
  vocabulary: { entries: [{ kind: "glossary_terms", count: 4 }] },
  registry: { entries: [] },
  strategy: { entries: [] },
  vol: { entries: [{ kind: "object_mappings", count: 3 }] },
  governance: { entries: [] },
  danglers: [
    {
      kind: "object_mappings",
      source_path: "ontology.vol.mappings[0]",
      missing_id: "CustomerV2",
    },
  ],
};

const AXIS_ITEMS_NODE_TYPE = [
  {
    id: "type-customer",
    label: "Customer",
    description: "End-user buyer",
  },
  {
    id: "type-order",
    label: "Order",
    description: "Placed purchase",
  },
  {
    id: "type-product",
    label: "Product",
    description: "Sold item",
  },
];

test.describe("schema → ontology proposal", () => {
  test.beforeEach(async ({ page }) => {
    await page.route(
      new RegExp(
        `/api/proxy/ontologies/${ONT_ID}/map-summary(\\?.*)?$`,
      ),
      async (route) => {
        await route.fulfill({
          status: 200,
          contentType: "application/json",
          body: JSON.stringify(MAP_SUMMARY),
        });
      },
    );
    await page.route(
      new RegExp(
        `/api/proxy/ontologies/${ONT_ID}/cross-refs(\\?.*)?$`,
      ),
      async (route) => {
        // `fetchCrossRefs` returns the raw `CrossRefEdge[]`; wrapping
        // it in `{ edges: [] }` makes `.map(...)` inside the flow
        // component throw "e is not iterable" and bricks the page.
        await route.fulfill({
          status: 200,
          contentType: "application/json",
          body: JSON.stringify([]),
        });
      },
    );
    await page.route(
      new RegExp(
        `/api/proxy/ontologies/${ONT_ID}/axis-items(\\?.*)?$`,
      ),
      async (route) => {
        await route.fulfill({
          status: 200,
          contentType: "application/json",
          body: JSON.stringify(AXIS_ITEMS_NODE_TYPE),
        });
      },
    );
  });

  test("renders axis cards with counts and the dangling-references callout", async ({
    page,
  }) => {
    const mapRequest = page.waitForRequest(
      (req) =>
        req
          .url()
          .includes(`/api/proxy/ontologies/${ONT_ID}/map-summary`) &&
        req.method() === "GET",
    );
    await page.goto(`/ontology/${ONT_ID}/map`);
    await mapRequest;

    // Dangling callout flags the missing `CustomerV2` id — the
    // fingerprint the bootstrap→design flow needs to resolve before
    // advancing.
    await expect(page.getByText(/CustomerV2/)).toBeVisible();

    // Topology axis tile renders the combined count `3+2=5` and the
    // vocabulary tile renders `4`.
    await expect(page.getByText("5").first()).toBeVisible();
    await expect(page.getByText("4").first()).toBeVisible();
  });

  test("clicking a NodeType count fires axis-items with the right kind and renders items", async ({
    page,
  }) => {
    await page.goto(`/ontology/${ONT_ID}/map`);
    await page.waitForLoadState("domcontentloaded");

    // Wait for the Topology card to render. Each axis card lists
    // its kinds as clickable rows; `node_types` renders via the
    // `ontology.map.axes.topology.kinds.node_types` bundle key as
    // "Node types".
    const nodeTypeRow = page
      .getByRole("button", { name: /Node types/i })
      .first();
    await expect(nodeTypeRow).toBeVisible();

    // Wait for the GET after clicking — capture the request so we
    // can assert the `kind` param.
    const axisRequest = page.waitForRequest(
      (req) =>
        req
          .url()
          .includes(`/api/proxy/ontologies/${ONT_ID}/axis-items`) &&
        req.method() === "GET",
    );
    await nodeTypeRow.click();
    const req = await axisRequest;
    // `URL.searchParams.get('kind')` is the contract between the
    // UI drill-down and the `ontology_axis_items_get_handler`.
    const url = new URL(req.url());
    expect(url.searchParams.get("kind")).toBe("node_types");

    // Modal renders the three Customer/Order/Product labels from
    // the mocked axis-items payload.
    await expect(page.getByText("Customer").first()).toBeVisible();
    await expect(page.getByText("Order").first()).toBeVisible();
    await expect(page.getByText("Product").first()).toBeVisible();
  });
});
