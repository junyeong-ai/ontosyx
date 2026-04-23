// Shared Playwright fixtures — locale pinning + baseline API mocks.
//
// Every spec that runs against the production build shares the same
// three concerns:
//
// 1. **Locale pinning.** `src/i18n/request.ts` reads
//    `ontosyx_locale` from a cookie; the default is `ko`. Most tests
//    were written against English copy, so we seed the cookie once
//    per browser context. Tests that want Korean output can call
//    `useLocale("ko")` from inside a `test.use({})` block.
//
// 2. **Workspace init.** `initWorkspace` (chrome-slice) calls
//    `GET /api/proxy/workspaces`. If it 502s (no backend running),
//    the slice still flips `workspaceReady = true` so the layout
//    unblocks, but some downstream hooks trigger bad requests. Seed
//    a cheap default workspace so the init path takes the happy fork.
//
// 3. **Auth probe.** `useAuth` calls `/auth/me`. In CI-proxy mode
//    that returns 401 and the app renders unauthenticated. Seeding a
//    fake user keeps admin-only UIs reachable from the tests.
//
// The extended `test` / `expect` here are drop-in replacements for
// the upstream ones:
//
// ```ts
// import { test, expect } from "./fixtures";
// ```
//
// If you deliberately want an unauthenticated / Korean-locale test,
// use `rawTest` instead and roll your own setup.

import {
  test as rawTest,
  expect as rawExpect,
  type BrowserContext,
} from "@playwright/test";

export const expect = rawExpect;

// Raw test handle for specs that want to opt out of all fixtures —
// e.g., korean-query which must render Korean copy to exercise the
// CJK rendering path.
export { rawTest };

/** Seed fake cookies + the auth-me stub into a fresh context. */
async function seedContext(
  context: BrowserContext,
  opts: { locale?: "en" | "ko" } = {},
): Promise<void> {
  const locale = opts.locale ?? "en";
  // `pnpm start` serves on localhost:3100 by default; scope the cookie
  // to localhost so it applies regardless of port drift.
  await context.addCookies([
    {
      name: "ontosyx_locale",
      value: locale,
      url: "http://localhost:3100",
    },
  ]);

  // Mark the welcome/onboarding modal as already dismissed. It's a
  // `localStorage`-gated overlay that steals pointer events from the
  // rest of the page until the user clicks through. Every test starts
  // in a fresh context (empty storage), so without this seed every
  // `.click()` races against the modal backdrop.
  await context.addInitScript(() => {
    try {
      window.localStorage.setItem("ontosyx.onboarded", "true");
    } catch {
      // Private mode / disabled — the modal may flash, but so would
      // every other localStorage-gated feature, so we don't special-case.
    }
  });

  // Baseline API stubs — every context shares them so tests don't
  // re-declare the same three boilerplate mocks. Per-test routes
  // added via `page.route` still override these.
  await context.route("**/auth/me", async (route) => {
    if (route.request().method() !== "GET") return route.fallback();
    await route.fulfill({
      status: 200,
      contentType: "application/json",
      body: JSON.stringify({
        sub: "test-user",
        email: "test@ontosyx.local",
        name: "Test User",
        role: "admin",
        auth_enabled: true,
      }),
    });
  });

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
          role: "admin",
          primary_locale: "ko",
          locale_fallback: ["ko", "en"],
          created_at: "2026-04-22T00:00:00Z",
        },
      ]),
    });
  });
}

/**
 * Extended `test` with the locale + auth + workspace fixtures wired.
 *
 * The locale default is `en`. Override per-test with:
 *
 * ```ts
 * test.use({ locale: "ko" });
 * ```
 */
export const test = rawTest.extend<{
  /** Resolves after the browser context has been seeded. */
  seededContext: void;
  /** The locale cookie value for this test. */
  locale: "en" | "ko";
}>({
  // `locale` is a regular test-scoped fixture with a default value.
  // Specs opt into a different locale via `test.use({ locale: "ko" })`,
  // which Playwright resolves BEFORE the `seededContext` fixture runs.
  locale: "en",
  seededContext: [
    async ({ context, locale }, use) => {
      await seedContext(context, { locale });
      await use();
    },
    // `auto: true` runs the fixture for every test, no matter whether
    // the test names it. Keeps the seed cost invisible at the spec
    // level so authors don't accidentally skip it.
    { auto: true },
  ],
});
