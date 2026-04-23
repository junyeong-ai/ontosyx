import { defineConfig, devices } from "@playwright/test";

/**
 * Playwright config — Phase 6.4 frontend E2E.
 *
 * Assumptions:
 * - Tests run against the production Next build (`pnpm build && pnpm start`)
 *   on port 3100. We avoid `pnpm dev` because HMR, react-refresh, and
 *   development-only error overlays diverge from what real users see.
 * - Backend API is proxied through Next at the same origin.
 * - Tests live under `web/tests/e2e/`.
 */
export default defineConfig({
  testDir: "./tests/e2e",
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  workers: process.env.CI ? 1 : undefined,
  reporter: process.env.CI ? [["github"], ["list"]] : "list",
  use: {
    baseURL: process.env.PLAYWRIGHT_BASE_URL ?? "http://localhost:3100",
    trace: "on-first-retry",
    video: "retain-on-failure",
    screenshot: "only-on-failure",
  },
  projects: [
    // Chromium only. The frontend is vanilla React + Tailwind — there
    // is no browser-specific code path (no IE-era hacks, no
    // Firefox-only IntersectionObserver quirks) so firefox parity
    // wouldn't buy real regression coverage, just 2× the CI time.
    // If we ever ship a feature that touches engine-specific behavior
    // (MediaRecorder, custom element polyfills, …), add the targeted
    // engines back here.
    {
      name: "chromium",
      use: { ...devices["Desktop Chrome"] },
    },
  ],
  webServer: process.env.PLAYWRIGHT_NO_SERVER
    ? undefined
    : {
        // `pnpm start` serves the production build and is closer to what
        // users see (no HMR, minified bundles, production error boundaries).
        // Local runs: prime once via `pnpm build` before `pnpm exec playwright test`.
        // CI pipelines should run `pnpm build` in a separate step so the
        // web server boot is fast enough to fit the 120s timeout.
        command: process.env.PLAYWRIGHT_SERVER_COMMAND ?? "pnpm start",
        url: "http://localhost:3100",
        reuseExistingServer: !process.env.CI,
        timeout: 120_000,
        env: {
          PORT: "3100",
        },
      },
});
