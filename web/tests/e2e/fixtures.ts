// Shared Playwright fixtures — locale pinning + baseline API mocks.
//
// Every spec that runs against the production build shares the same
// four concerns:
//
// 1. **Locale pinning.** `src/i18n/request.ts` reads
//    `ontosyx_locale` from a cookie; the default is `ko`. Most tests
//    were written against English copy, so we seed the cookie once
//    per browser context. Tests that want Korean output override
//    with `test.use({ locale: "ko" })`.
//
// 2. **Role pinning.** `useAuth` calls `/auth/me`. The baseline
//    stub returns an `admin` user so admin-only UI stays reachable;
//    specs that exercise non-admin paths override with
//    `test.use({ role: "designer" })` or `"viewer"`. This is the
//    single toggle — never mock the backend's `require_admin()`
//    behaviour; go through the same `/auth/me` seam real UIs use.
//
// 3. **Workspace init.** `initWorkspace` (chrome-slice) calls
//    `GET /api/proxy/workspaces`. If it 502s (no backend running),
//    the slice still flips `workspaceReady = true` so the layout
//    unblocks, but downstream hooks issue bad requests. Seeding a
//    default workspace forces the init path onto the happy fork.
//
// 4. **Onboarding modal dismissal.** `WelcomeModal` uses a
//    localStorage-gated overlay that steals pointer events until
//    clicked through. Every Playwright context starts with empty
//    storage, so we pre-seed `ontosyx.onboarded`.
//
// The extended `test` / `expect` here are drop-in replacements for
// the upstream ones:
//
// ```ts
// import { test, expect } from "./fixtures";
// ```
//
// If a spec must run WITHOUT any of the seeds (e.g., testing the
// onboarding modal itself), use `rawTest` and roll your own setup.

import {
  test as rawTest,
  expect as rawExpect,
  type BrowserContext,
} from "@playwright/test";

export const expect = rawExpect;

// Raw test handle for specs that want to opt out of all fixtures —
// e.g., onboarding-modal tests that need the modal to actually show.
export { rawTest };

/** Roles the backend's `Principal` middleware recognises. Kept as
 *  an exported union so specs can use `test.use({ role: ... })`
 *  with autocomplete support. `"none"` short-circuits the auth-me
 *  mock to a 401 — use it for unauthenticated flows. */
export type SeedRole = "admin" | "designer" | "viewer" | "none";

/** Profile payload shape the seeded `/auth/me` mock returns. Only
 *  the fields `useAuth` actually reads. */
interface AuthMeProfile {
  sub: string;
  email: string;
  name: string;
  role: Exclude<SeedRole, "none">;
  auth_enabled: boolean;
}

function buildAuthMeProfile(role: Exclude<SeedRole, "none">): AuthMeProfile {
  return {
    sub: `test-${role}`,
    email: `test.${role}@ontosyx.local`,
    // Role is visible in user-facing "signed in as" labels; keeping
    // the name in sync means assertions can grep for "Test Admin"
    // etc. without test-specific mocks.
    name: `Test ${role.charAt(0).toUpperCase()}${role.slice(1)}`,
    role,
    auth_enabled: true,
  };
}

/** Seed cookies + baseline API stubs into a fresh context. */
async function seedContext(
  context: BrowserContext,
  opts: { locale: "en" | "ko"; role: SeedRole },
): Promise<void> {
  // Locale cookie scoped to localhost; the production build serves
  // on :3100 by default.
  await context.addCookies([
    {
      name: "ontosyx_locale",
      value: opts.locale,
      url: "http://localhost:3100",
    },
  ]);

  // Mark the welcome/onboarding modal as already dismissed. Without
  // this seed every first `.click()` would race against the modal
  // backdrop.
  await context.addInitScript(() => {
    try {
      window.localStorage.setItem("ontosyx.onboarded", "true");
    } catch {
      // Private mode / disabled — the modal may flash, but so would
      // every other localStorage-gated feature, so we don't special-case.
    }
  });

  // `/auth/me` — the single source the UI reads to decide whether
  // the viewer is admin / designer / viewer / logged-out. Role flows
  // through here, not through backend stubs.
  await context.route("**/auth/me", async (route) => {
    if (route.request().method() !== "GET") return route.fallback();
    if (opts.role === "none") {
      await route.fulfill({
        status: 401,
        contentType: "application/json",
        body: JSON.stringify({ error: "unauthenticated" }),
      });
      return;
    }
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify(buildAuthMeProfile(opts.role)),
    });
  });

  // Workspace list — the chrome-slice reads `role` from here to
  // decide what the sidebar and action rails are allowed to do. The
  // per-workspace role mirrors the auth-me role so the two sources
  // stay internally consistent.
  const workspaceRole: SeedRole = opts.role === "none" ? "viewer" : opts.role;
  await context.route("**/api/proxy/workspaces", async (route) => {
    if (route.request().method() !== "GET") return route.fallback();
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify([
        {
          id: "00000000-0000-0000-0000-000000000000",
          name: "Default",
          slug: "default",
          role: workspaceRole,
          primary_locale: "ko",
          admin_locale_fallback: ["ko", "en"],
          llm_locale_fallback: ["en", "ko"],
          created_at: "2026-04-22T00:00:00Z",
        },
      ]),
    });
  });
}

/**
 * Extended `test` with locale + role + workspace + onboarding
 * fixtures wired.
 *
 * Defaults: `locale = "en"`, `role = "admin"` — the combination most
 * existing specs expect. Override per-describe / per-test:
 *
 * ```ts
 * test.use({ role: "viewer" });           // unauthenticated-adjacent flows
 * test.use({ locale: "ko", role: "designer" });
 * ```
 */
export const test = rawTest.extend<{
  /** Resolves after the browser context has been seeded. */
  seededContext: void;
  /** The locale cookie value for this test. */
  locale: "en" | "ko";
  /** The role the seeded `/auth/me` mock advertises. `"none"`
   *  returns 401 so the UI renders as logged-out. */
  role: SeedRole;
}>({
  locale: "en",
  role: "admin",
  seededContext: [
    async ({ context, locale, role }, use) => {
      await seedContext(context, { locale, role });
      await use();
    },
    // `auto: true` runs the fixture for every test, no matter
    // whether the test names it. Keeps the seed cost invisible at
    // the spec level so authors don't accidentally skip it.
    { auto: true },
  ],
});
