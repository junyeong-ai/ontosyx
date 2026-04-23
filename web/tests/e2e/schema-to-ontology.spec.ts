import { test, expect } from "@playwright/test";

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

const MAP_SUMMARY = {
  ontology_id: ONT_ID,
  version: "v2",
  topology: {
    entries: [
      { kind: "NodeType", count: 3 },
      { kind: "EdgeType", count: 2 },
    ],
  },
  vocabulary: { entries: [{ kind: "GlossaryTerm", count: 4 }] },
  registry: { entries: [] },
  strategy: { entries: [] },
  vol: { entries: [{ kind: "ObjectMapping", count: 3 }] },
  governance: { entries: [] },
  danglers: [
    {
      kind: "ObjectMapping",
      source_path: "ontology.vol.mappings[0]",
      missing_id: "CustomerV2",
    },
  ],
};

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
        await route.fulfill({
          status: 200,
          contentType: "application/json",
          body: JSON.stringify({ edges: [] }),
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
});
