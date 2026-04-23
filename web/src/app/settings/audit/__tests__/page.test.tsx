import { describe, it, expect, vi, beforeEach } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { NextIntlClientProvider } from "next-intl";
import type { ReactElement } from "react";

import messages from "../../../../../messages/en.json";

vi.mock("@/lib/api/client", () => ({
  request: vi.fn(),
}));

vi.mock("sonner", () => ({
  toast: { success: vi.fn(), error: vi.fn() },
}));

import AuditSettingsPage from "@/app/settings/audit/page";
import { request } from "@/lib/api/client";
import { toast } from "sonner";

const SAMPLE_ENTRY = {
  id: "audit-1",
  user_id: "00000000-0000-0000-0000-000000000001",
  action: "create_project",
  resource_type: "project",
  resource_id: "00000000-0000-0000-0000-000000000abc",
  details: {},
  created_at: "2026-04-23T09:00:00Z",
};

function renderPage(): void {
  const ui: ReactElement = (
    <NextIntlClientProvider locale="en" messages={messages}>
      <AuditSettingsPage />
    </NextIntlClientProvider>
  );
  render(ui);
}

describe("AuditSettingsPage", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    (request as ReturnType<typeof vi.fn>).mockReset();
    (toast.success as ReturnType<typeof vi.fn>).mockReset();
    (toast.error as ReturnType<typeof vi.fn>).mockReset();
  });

  it("renders the audit table and formats snake_case actions as Title Case", async () => {
    (request as ReturnType<typeof vi.fn>).mockResolvedValueOnce({
      items: [SAMPLE_ENTRY],
    });
    renderPage();
    await waitFor(() =>
      expect(screen.getByText("Create Project")).toBeInTheDocument(),
    );
    // `system` label replaces a null user_id — only renders when
    // the entry has `user_id === null`, our sample sets a uuid so
    // we check the system label is NOT present here.
    expect(screen.queryByText("system")).not.toBeInTheDocument();
  });

  it("shows empty-state row when the api returns zero entries", async () => {
    (request as ReturnType<typeof vi.fn>).mockResolvedValueOnce({
      items: [],
    });
    renderPage();
    await waitFor(() =>
      expect(
        screen.getByText(/No audit entries for the selected period/),
      ).toBeInTheDocument(),
    );
  });

  it("changing the date filter refetches with a fresh `from` window", async () => {
    (request as ReturnType<typeof vi.fn>).mockResolvedValue({ items: [] });
    renderPage();
    // Initial load fires once with days=30.
    await waitFor(() =>
      expect(request).toHaveBeenCalledTimes(1),
    );
    const firstCallUrl = (request as ReturnType<typeof vi.fn>).mock
      .calls[0][0] as string;
    expect(firstCallUrl).toMatch(/^\/audit\?from=/);

    // Switch to 7 days — the `useCallback` dependency changes +
    // the effect refires.
    fireEvent.change(screen.getByRole("combobox"), {
      target: { value: "7" },
    });
    await waitFor(() =>
      expect(request).toHaveBeenCalledTimes(2),
    );
    const secondCallUrl = (request as ReturnType<typeof vi.fn>).mock
      .calls[1][0] as string;
    expect(secondCallUrl).toMatch(/^\/audit\?from=/);
    // The `from` timestamp differs — confirm the two calls
    // weren't a cache hit on the same window.
    expect(secondCallUrl).not.toEqual(firstCallUrl);
  });

  it("null user_id renders the `system` fallback label", async () => {
    (request as ReturnType<typeof vi.fn>).mockResolvedValueOnce({
      items: [{ ...SAMPLE_ENTRY, user_id: null }],
    });
    renderPage();
    await waitFor(() =>
      expect(screen.getByText("system")).toBeInTheDocument(),
    );
  });
});
