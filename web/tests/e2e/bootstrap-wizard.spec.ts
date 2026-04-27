import { test, expect } from "./fixtures";
import { BOOTSTRAP_STORAGE_KEY } from "@/app/bootstrap/bootstrap-state";

/**
 * Phase 5 — Bootstrap wizard happy path.
 *
 * Walks a user from `/bootstrap/1-pilot` through the six wizard
 * steps and clicks Finish on the validate screen. The Finish
 * handler makes two backend calls:
 *
 *   1. `POST /api/ontologies` — the unified creation endpoint.
 *      Fires when the operator entered non-empty glossary drafts;
 *      the wizard converts them into `CreateGlossaryTerm` edit ops
 *      and posts them as `initial_operations`. Mocked here to
 *      return a fresh ontology id + version number.
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
const MOCK_VERSION_ID = "00000000-0000-0000-0000-0000000000c3";
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
    await page.addInitScript((key: string) => {
      window.localStorage.removeItem(key);
    }, BOOTSTRAP_STORAGE_KEY);

    // `/api/proxy/ontologies` serves both the pre-flight name
    // lookup (GET with `?name_eq=...`) and the unified creation POST.
    // Default route: empty-list GET + success POST; individual tests
    // override either branch when the scenario needs it.
    await page.route(/\/api\/proxy\/ontologies(\?.*)?$/, async (route) => {
      const req = route.request();
      if (req.method() === "GET") {
        await route.fulfill({
          status: 200,
          contentType: "application/json",
          body: JSON.stringify({ items: [] }),
        });
      } else if (req.method() === "POST") {
        const body = req.postDataJSON() as {
          initial_operations?: unknown[];
        };
        const applied = Array.isArray(body.initial_operations)
          ? body.initial_operations.length
          : 0;
        await route.fulfill({
          status: 201,
          contentType: "application/json",
          body: JSON.stringify({
            ontology_id: MOCK_ONTOLOGY_ID,
            version_id: MOCK_VERSION_ID,
            version: 1,
            applied_operations: applied,
            committed_at: new Date().toISOString(),
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
    await page.getByRole("button", { name: /^(?:next|다음)$/i }).click();
    await expect(page).toHaveURL(/\/bootstrap\/2-source$/);

    // Step 2 — pick a connection-based source. The source-kind
    // control is a visually-hidden radio group (`input[type='radio']
    // .sr-only`) wrapped by labels; Playwright can toggle the input
    // directly by value. `role="radio"` resolves via the input, not
    // the label, so `.check()` works end-to-end.
    // The radio input itself is `.sr-only` (visually hidden); the
    // click target is the wrapping label. `.check({ force: true })`
    // skips the visibility gate since the input remains interactive
    // for assistive tech even when display-hidden.
    await page
      .getByRole("radio", { name: /postgresql/i })
      .check({ force: true });
    const connInput = page.getByPlaceholder(/postgres|connection|url/i).first();
    await connInput.fill("postgresql://localhost:5432/pilot");

    await page.getByRole("button", { name: /^(?:next|다음)$/i }).click();
    await expect(page).toHaveURL(/\/bootstrap\/2b-select-tables$/);

    // Step 2b — keep the default "all" mode and advance.
    await page.getByRole("button", { name: /^(?:next|다음)$/i }).click();
    await expect(page).toHaveURL(/\/bootstrap\/3-glossary$/);
  });

  test("Finish fires ontology-create + createProject and redirects to /design", async ({
    page,
  }) => {
    // Wait for the mocked POSTs so the asserts can verify the
    // exact request payloads fired, not just that the redirect
    // happened.
    const createOntologyRequest = page.waitForRequest(
      (req) =>
        /\/api\/proxy\/ontologies(\?.*)?$/.test(req.url()) &&
        req.method() === "POST",
    );
    const projectRequest = page.waitForRequest(
      (req) =>
        /\/api\/proxy\/projects(\?.*)?$/.test(req.url()) &&
        req.method() === "POST",
    );

    // --- Step 1: pilot name + scope -------------------------
    await page.goto("/bootstrap/1-pilot");
    await page
      .locator("input[type='text'], input:not([type])")
      .first()
      .fill("E2E Pilot");
    await page.getByRole("button", { name: /^(?:next|다음)$/i }).click();
    await expect(page).toHaveURL(/\/bootstrap\/2-source$/);

    // --- Step 2: source = postgres + connection -----------
    // The radio input itself is `.sr-only` (visually hidden); the
    // click target is the wrapping label. `.check({ force: true })`
    // skips the visibility gate since the input remains interactive
    // for assistive tech even when display-hidden.
    await page
      .getByRole("radio", { name: /postgresql/i })
      .check({ force: true });
    await page
      .getByPlaceholder(/postgres|connection|url/i)
      .first()
      .fill("postgresql://localhost:5432/pilot");
    await page.getByRole("button", { name: /^(?:next|다음)$/i }).click();
    await expect(page).toHaveURL(/\/bootstrap\/2b-select-tables$/);

    // --- Step 2b: keep default "all" mode -----------------
    await page.getByRole("button", { name: /^(?:next|다음)$/i }).click();
    await expect(page).toHaveURL(/\/bootstrap\/3-glossary$/);

    // --- Step 3: glossary draft (feeds CreateGlossaryTerm ops) ----
    // Two terms with descriptions — the parser collapses into two
    // `GlossaryTermDraft` rows; the Finish handler maps each to a
    // `{ op: "create_glossary_term", def: { ... } }` op.
    const glossaryTextarea = page.locator("#glossary-draft");
    await glossaryTextarea.fill(
      "Customer: a buyer of goods\nOrder: a placed purchase\n",
    );
    await page.getByRole("button", { name: /^(?:next|다음)$/i }).click();
    await expect(page).toHaveURL(/\/bootstrap\/4-rules$/);

    // --- Step 4: rules draft (optional content; skip) -----
    await page.getByRole("button", { name: /^(?:next|다음)$/i }).click();
    await expect(page).toHaveURL(/\/bootstrap\/5-map$/);

    // --- Step 5: mapping notes (optional; skip) -----------
    await page.getByRole("button", { name: /^(?:next|다음)$/i }).click();
    await expect(page).toHaveURL(/\/bootstrap\/6-validate$/);

    // --- Step 6: Finish ---------------------------------
    // The StepShell renders a "Finish" button on the last step
    // (nextPath === null). Match "Finish" or its ko
    // equivalent "완료".
    await page.getByRole("button", { name: /^finish$|완료/i }).click();

    // Both backend calls fire — wait on each, then assert the
    // payloads carry what the wizard captured.
    const [createOntology, project] = await Promise.all([
      createOntologyRequest,
      projectRequest,
    ]);

    const createBody = createOntology.postDataJSON() as {
      name: string;
      initial_operations: Array<{
        op: string;
        def: { term: string; aliases: string[] };
      }>;
    };
    expect(createBody.name).toBe("E2E Pilot");
    // Every op uses the canonical `create_glossary_term` discriminator
    // + `def` shape from the unified `OntologyEditOp` vocabulary.
    expect(createBody.initial_operations.map((o) => o.op)).toEqual([
      "create_glossary_term",
      "create_glossary_term",
    ]);
    expect(
      createBody.initial_operations.map((o) => o.def.term),
    ).toEqual(["Customer", "Order"]);

    const projectBody = project.postDataJSON() as {
      title: string;
      origin_type: string;
      source: { type: string; connection_string: string };
    };
    expect(projectBody.title).toBe("E2E Pilot");
    expect(projectBody.origin_type).toBe("source");
    expect(projectBody.source.type).toBe("postgresql");
    expect(projectBody.source.connection_string).toBe(
      "postgresql://localhost:5432/pilot",
    );

    // Finally — the page redirects to /design with the new
    // project id on the query string.
    await expect(page).toHaveURL(
      new RegExp(`/design\\?project=${MOCK_PROJECT.id}`),
    );
  });

  test("Finish surfaces ExistingPilotDialog when the name already exists", async ({
    page,
  }) => {
    // Override the default empty-list GET with a single matching row
    // — as if a prior session had committed a pilot under the same
    // name. The POST branch stays on the default (success) so a
    // regression would still fire a stray create.
    const EXISTING_ID = "00000000-0000-0000-0000-0000000000e1";
    await page.route(/\/api\/proxy\/ontologies(\?.*)?$/, async (route) => {
      const req = route.request();
      if (req.method() === "GET") {
        await route.fulfill({
          status: 200,
          contentType: "application/json",
          body: JSON.stringify({
            items: [
              {
                id: EXISTING_ID,
                lineage_id: "lineage-existing",
                name: "E2E Pilot",
                description: { default: "", translations: {} },
                created_at: "2026-04-22T00:00:00Z",
                updated_at: "2026-04-22T00:00:00Z",
              },
            ],
          }),
        });
      } else {
        await route.fallback();
      }
    });

    // Seed wizard state directly rather than walking all 6 steps —
    // we're testing the collision branch, not the happy path.
    await page.addInitScript((key: string) => {
      window.localStorage.setItem(
        key,
        JSON.stringify({
          pilotName: "E2E Pilot",
          pilotScope: "repeat run",
          sourceKind: "postgresql",
          sourceConnection: "postgresql://localhost:5432/pilot",
          glossaryDraft: "Customer: buyer",
          rulesDraft: "",
          mappingNotes: "",
          completedSteps: [
            "1-pilot",
            "2-source",
            "3-glossary",
            "4-rules",
            "5-map",
          ],
        }),
      );
    }, BOOTSTRAP_STORAGE_KEY);

    await page.goto("/bootstrap/6-validate");

    // Wait for the wizard state to hydrate from localStorage before
    // clicking Finish — the bootstrap provider uses
    // useSyncExternalStore with a microtask-deferred hydration, so
    // the first render briefly shows empty state.
    await expect(page.getByRole("heading", { level: 3 })).toContainText(
      "E2E Pilot",
    );

    // Clicking Finish triggers the pre-flight lookup → match →
    // dialog opens before any POST fires. Assert that the dialog is
    // visible with the colliding name and the suggested rename, and
    // that no ontology POST was made.
    let postCalls = 0;
    await page.route(/\/api\/proxy\/ontologies(\?.*)?$/, async (route) => {
      if (route.request().method() === "POST") {
        postCalls += 1;
      }
      await route.fallback();
    });

    await page.getByRole("button", { name: /^finish$|완료/i }).click();

    const dialog = page.getByTestId("existing-pilot-dialog");
    await expect(dialog).toBeVisible();
    await expect(dialog).toContainText(/E2E Pilot/);
    await expect(dialog).toContainText(/E2E Pilot 2/);

    // Click "Continue" — the page should redirect to the existing
    // ontology's map page without firing a create POST.
    await page.getByTestId("existing-pilot-continue").click();

    await expect(page).toHaveURL(
      new RegExp(`/ontology/${EXISTING_ID}/map$`),
    );
    expect(postCalls).toBe(0);
  });
});
