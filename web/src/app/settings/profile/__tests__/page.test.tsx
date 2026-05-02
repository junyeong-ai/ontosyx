import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import { NextIntlClientProvider } from "next-intl";
import type { ReactElement } from "react";

import messages from "../../../../../messages/en.json";

vi.mock("@/hooks/use-auth", () => ({
  useAuth: vi.fn(),
}));

import ProfilePage from "@/app/settings/profile/page";
import { useAuth } from "@/hooks/use-auth";

function renderPage(): void {
  const ui: ReactElement = (
    <NextIntlClientProvider locale="en" messages={messages}>
      <ProfilePage />
    </NextIntlClientProvider>
  );
  render(ui);
}

describe("ProfilePage", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it("renders the dev-mode placeholder when auth is disabled", async () => {
    const devUser = {
      sub: "dev",
      email: "dev@local",
      name: "Developer",
      role: "admin",
      auth_enabled: false,
    } as const;
    vi.mocked(useAuth).mockReturnValue({
      mode: { kind: "disabled", user: devUser },
      user: devUser,
      loading: false,
      isAuthenticated: true,
      authEnabled: false,
      isAdmin: true,
      canWrite: true,
    });
    renderPage();
    await waitFor(() =>
      expect(screen.getByText("Developer")).toBeInTheDocument(),
    );
    expect(screen.getByText(/Development mode/)).toBeInTheDocument();
  });

  it("renders the not-signed-in card when authEnabled but no user", async () => {
    vi.mocked(useAuth).mockReturnValue({
      mode: { kind: "unauthenticated" },
      user: null,
      loading: false,
      isAuthenticated: false,
      authEnabled: true,
      isAdmin: false,
      canWrite: false,
    });
    renderPage();
    await waitFor(() =>
      expect(screen.getByText("Not signed in.")).toBeInTheDocument(),
    );
    // The Sign in CTA links to /login.
    const cta = screen.getByRole("link");
    expect(cta).toHaveAttribute("href", "/login");
  });

  it("renders identity + role + sign-out form for an authenticated user", async () => {
    const aliceUser = {
      sub: "u-1",
      email: "alice@example.com",
      name: "Alice",
      role: "designer",
      auth_enabled: true,
    } as const;
    vi.mocked(useAuth).mockReturnValue({
      mode: { kind: "authenticated", user: aliceUser },
      user: aliceUser,
      loading: false,
      isAuthenticated: true,
      authEnabled: true,
      isAdmin: false,
      canWrite: true,
    });
    renderPage();
    // Name appears in both the avatar fallback and the identity card.
    await waitFor(() =>
      expect(screen.getAllByText("Alice").length).toBeGreaterThan(0),
    );
    expect(screen.getAllByText("alice@example.com").length).toBeGreaterThan(0);
    // Role pill renders translated label.
    expect(screen.getAllByText("Designer").length).toBeGreaterThan(0);
    // Sign Out form posts to /auth/logout.
    const form = screen
      .getByRole("button", { name: /Sign Out/ })
      .closest("form");
    expect(form).toHaveAttribute("action", "/auth/logout");
    expect(form).toHaveAttribute("method", "POST");
  });

  it("falls back to the raw role string when the role has no translation", async () => {
    const bobUser = {
      sub: "u-2",
      email: "bob@example.com",
      name: "Bob",
      role: "auditor",
      auth_enabled: true,
    } as const;
    vi.mocked(useAuth).mockReturnValue({
      mode: { kind: "authenticated", user: bobUser },
      user: bobUser,
      loading: false,
      isAuthenticated: true,
      authEnabled: true,
      isAdmin: false,
      canWrite: false,
    });
    renderPage();
    await waitFor(() =>
      expect(screen.getAllByText("Bob").length).toBeGreaterThan(0),
    );
    // "auditor" has no translation, so the page displays the raw string.
    expect(screen.getAllByText("auditor").length).toBeGreaterThan(0);
  });
});
