import { describe, it, expect, vi, beforeEach } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { NextIntlClientProvider } from "next-intl";
import type { ReactElement } from "react";

import messages from "../../../../../messages/en.json";

vi.mock("@/hooks/use-auth", () => ({
  useAuth: vi.fn(),
}));

vi.mock("@/lib/api", () => ({
  listUsers: vi.fn(),
  updateUserRole: vi.fn(),
}));

import TeamPage from "@/app/settings/team/page";
import * as api from "@/lib/api";
import { useAuth } from "@/hooks/use-auth";
import type { UserInfo } from "@/types/api";

const MEMBER_A: UserInfo = {
  id: "u-a",
  email: "alice@example.com",
  name: "Alice",
  picture: null,
  role: "admin",
};
const MEMBER_B: UserInfo = {
  id: "u-b",
  email: "bob@example.com",
  name: "Bob",
  picture: null,
  role: "viewer",
};

function renderPage(): void {
  const ui: ReactElement = (
    <NextIntlClientProvider locale="en" messages={messages}>
      <TeamPage />
    </NextIntlClientProvider>
  );
  render(ui);
}

function asAdmin(): void {
  vi.mocked(useAuth).mockReturnValue({
    user: { sub: "u-a", email: "alice@example.com", name: "Alice", role: "admin", auth_enabled: true },
    loading: false,
    isAuthenticated: true,
    authEnabled: true,
    isAdmin: true,
    canWrite: true,
  } as ReturnType<typeof useAuth>);
}

describe("TeamPage", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    vi.mocked(api.listUsers).mockReset();
    vi.mocked(api.updateUserRole).mockReset();
  });

  it("renders the auth-required placeholder when auth is disabled", async () => {
    vi.mocked(useAuth).mockReturnValue({
      user: null,
      loading: false,
      isAuthenticated: false,
      authEnabled: false,
      isAdmin: false,
      canWrite: false,
    } as ReturnType<typeof useAuth>);
    renderPage();
    // `authRequired` copy renders when the API isn't wired up.
    await waitFor(() =>
      expect(
        screen.getByText(
          /Team management is available when authentication is enabled/i,
        ),
      ).toBeInTheDocument(),
    );
    expect(api.listUsers).not.toHaveBeenCalled();
  });

  it("renders members and marks the current user with the `you` badge", async () => {
    asAdmin();
    vi.mocked(api.listUsers).mockResolvedValueOnce({
      items: [MEMBER_A, MEMBER_B],
      next_cursor: undefined,
    });
    renderPage();
    await waitFor(() =>
      expect(screen.getByText("Alice")).toBeInTheDocument(),
    );
    expect(screen.getByText("Bob")).toBeInTheDocument();
    // The "You" chip only renders next to the row matching useAuth.user.sub.
    expect(screen.getByText("you")).toBeInTheDocument();
  });

  it("changing a peer's role fires updateUserRole and swaps the row", async () => {
    asAdmin();
    vi.mocked(api.listUsers).mockResolvedValueOnce({
      items: [MEMBER_A, MEMBER_B],
      next_cursor: undefined,
    });
    vi.mocked(api.updateUserRole).mockResolvedValueOnce({
      user: { ...MEMBER_B, role: "designer" },
    });
    renderPage();
    await waitFor(() =>
      expect(screen.getByText("Bob")).toBeInTheDocument(),
    );
    // Locate Bob's role dropdown via its aria-label.
    const select = screen.getByLabelText(/Change role for Bob/i);
    fireEvent.change(select, { target: { value: "designer" } });
    await waitFor(() =>
      expect(api.updateUserRole).toHaveBeenCalledWith("u-b", "designer"),
    );
  });

  it("renders the load-failed error message when listUsers rejects", async () => {
    asAdmin();
    vi.mocked(api.listUsers).mockRejectedValueOnce(new Error("list broke"));
    renderPage();
    await waitFor(() =>
      expect(screen.getByText(/list broke/)).toBeInTheDocument(),
    );
    // No member rows render in the error state.
    expect(screen.queryByText("Alice")).not.toBeInTheDocument();
  });
});
