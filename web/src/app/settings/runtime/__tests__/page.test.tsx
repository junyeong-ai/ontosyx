import { describe, it, expect, vi, beforeEach } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { NextIntlClientProvider } from "next-intl";
import type { ReactElement } from "react";

import messages from "../../../../../messages/en.json";

vi.mock("@/hooks/use-auth", () => ({
  useAuth: vi.fn(),
}));

vi.mock("@/lib/api", () => ({
  getConfig: vi.fn(),
  updateConfig: vi.fn(),
}));

vi.mock("@/components/ui/toast", () => ({
  toast: { success: vi.fn(), error: vi.fn() },
}));

import SystemSettingsPage from "@/app/settings/runtime/page";
import * as api from "@/lib/api";
import { useAuth } from "@/hooks/use-auth";
import { mockAuth } from "@/test-utils/auth";
import { toast } from "@/components/ui/toast";

const UI_CONFIG = {
  ui: [
    {
      key: "theme",
      value: "dark",
      data_type: "string",
      description: "Default theme",
    },
  ],
  thresholds: [
    {
      key: "max_hops",
      value: "3",
      data_type: "int",
      description: "Maximum traversal depth",
    },
  ],
};

function renderPage(): void {
  const ui: ReactElement = (
    <NextIntlClientProvider locale="en" messages={messages}>
      <SystemSettingsPage />
    </NextIntlClientProvider>
  );
  render(ui);
}

function asAdmin(): void {
  vi.mocked(useAuth).mockReturnValue(
    mockAuth(
      { kind: "authenticated", role: "admin" },
      { sub: "u1", email: "a@b.c", name: "Admin" },
    ),
  );
}

describe("SystemSettingsPage", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    vi.mocked(api.getConfig).mockReset();
    vi.mocked(api.updateConfig).mockReset();
    (toast.success as ReturnType<typeof vi.fn>).mockReset();
    (toast.error as ReturnType<typeof vi.fn>).mockReset();
  });

  it("renders config entries grouped by category after getConfig resolves", async () => {
    asAdmin();
    vi.mocked(api.getConfig).mockResolvedValueOnce(UI_CONFIG);
    renderPage();
    // The ui category's `theme` entry renders its current value.
    await waitFor(() =>
      expect(screen.getByDisplayValue("dark")).toBeInTheDocument(),
    );
  });

  it("editing an int field to a non-integer shows the validation toast on save", async () => {
    asAdmin();
    vi.mocked(api.getConfig).mockResolvedValueOnce(UI_CONFIG);
    renderPage();
    // Thresholds tab is not active by default — switch before editing.
    await waitFor(() =>
      expect(screen.getByDisplayValue("dark")).toBeInTheDocument(),
    );
    // TabBar uses Base UI `Tabs.Tab` which has role="tab".
    fireEvent.click(
      screen.getByRole("tab", { name: /Schema Thresholds/ }),
    );
    await waitFor(() =>
      expect(screen.getByDisplayValue("3")).toBeInTheDocument(),
    );
    // Type a non-integer.
    fireEvent.change(screen.getByDisplayValue("3"), {
      target: { value: "not-a-number" },
    });
    // Save button becomes enabled once values diverge — click it.
    fireEvent.click(screen.getByRole("button", { name: /^Save$/ }));
    await waitFor(() =>
      expect(toast.error).toHaveBeenCalled(),
    );
    // Bad value is rejected before any PUT fires.
    expect(api.updateConfig).not.toHaveBeenCalled();
  });

  it("valid edit + save calls updateConfig with the changed entry", async () => {
    asAdmin();
    vi.mocked(api.getConfig).mockResolvedValueOnce(UI_CONFIG);
    vi.mocked(api.updateConfig).mockResolvedValueOnce({ updated: 1 });
    // After save the page calls loadConfig again.
    vi.mocked(api.getConfig).mockResolvedValueOnce(UI_CONFIG);
    renderPage();
    await waitFor(() =>
      expect(screen.getByDisplayValue("dark")).toBeInTheDocument(),
    );
    fireEvent.change(screen.getByDisplayValue("dark"), {
      target: { value: "light" },
    });
    fireEvent.click(screen.getByRole("button", { name: /^Save$/ }));
    await waitFor(() =>
      expect(api.updateConfig).toHaveBeenCalledWith({
        updates: [{ category: "ui", key: "theme", value: "light" }],
      }),
    );
    expect(toast.success).toHaveBeenCalled();
  });

  it("disables Save for a non-admin even when edits are pending", async () => {
    vi.mocked(useAuth).mockReturnValue(
      mockAuth(
        { kind: "authenticated", role: "viewer" },
        { sub: "u1", email: "a@b.c", name: "Member" },
      ),
    );
    vi.mocked(api.getConfig).mockResolvedValueOnce(UI_CONFIG);
    renderPage();
    await waitFor(() =>
      expect(screen.getByDisplayValue("dark")).toBeInTheDocument(),
    );
    fireEvent.change(screen.getByDisplayValue("dark"), {
      target: { value: "light" },
    });
    // With `isAdmin=false`, the Save button stays disabled even
    // though the edit is tracked — a click should be a no-op.
    const saveBtn = screen.getByRole("button", { name: /^Save$/ });
    expect(saveBtn).toBeDisabled();
  });
});
