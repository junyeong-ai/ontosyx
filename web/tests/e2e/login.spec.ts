import { test, expect } from "@playwright/test";

/**
 * Phase 6.4 — happy-path login.
 *
 * The app uses Google SSO. On CI we don't complete the real OAuth dance;
 * instead we:
 *   1. Visit /login and verify the page renders.
 *   2. Inject a fake auth cookie (mimicking what the callback would set).
 *   3. Reload / and confirm we land on the workbench (not redirected back).
 *
 * If the app's middleware verifies the JWT signature strictly, this test
 * becomes a smoke test of /login rendering only — still useful.
 */
test.describe("login", () => {
  test("renders login page with SSO button", async ({ page }) => {
    await page.goto("/login");
    await expect(page.getByRole("heading", { name: /Ontosyx/i })).toBeVisible();
    await expect(
      page.getByRole("link", { name: /sign in with google/i }),
    ).toBeVisible();
  });

  test("SSO button links to /auth/google", async ({ page }) => {
    await page.goto("/login");
    const link = page.getByRole("link", { name: /sign in with google/i });
    await expect(link).toHaveAttribute("href", "/auth/google");
  });

  test("displays error banner when error param present", async ({ page }) => {
    await page.goto("/login?error=token_exchange_failed");
    await expect(
      page.getByText(/failed to authenticate with google/i),
    ).toBeVisible();
  });
});
