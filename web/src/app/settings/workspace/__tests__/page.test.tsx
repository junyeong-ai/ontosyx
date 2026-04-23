import { describe, it, expect, vi, beforeEach } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { NextIntlClientProvider } from "next-intl";
import type { ReactElement } from "react";

import messages from "../../../../../messages/en.json";

vi.mock("@/lib/api/workspaces", () => ({
  getWorkspace: vi.fn(),
  updateWorkspace: vi.fn(),
  updateWorkspaceLocale: vi.fn(),
  listMembers: vi.fn(),
}));

vi.mock("@/lib/workspace", () => ({
  getWorkspaceId: vi.fn(),
  setWorkspaceName: vi.fn(),
}));

// MembersTable lives in a sibling module; keep the test focused on
// workspace identity + locale fields only.
vi.mock("@/components/workspace/members-table", () => ({
  MembersTable: () => <div data-testid="members-table" />,
}));

vi.mock("sonner", () => ({
  toast: { success: vi.fn(), error: vi.fn() },
}));

import WorkspaceSettingsPage from "@/app/settings/workspace/page";
import * as api from "@/lib/api/workspaces";
import * as ws from "@/lib/workspace";
import type { Workspace } from "@/types/workspace";
import { toast } from "sonner";

const WS: Workspace = {
  id: "ws-1",
  name: "Acme analytics",
  slug: "acme",
  owner_id: "u-a",
  settings: {},
  primary_locale: "ko",
  locale_fallback: ["ko", "en"],
  created_at: "2026-04-22T00:00:00Z",
};

function renderPage(): void {
  const ui: ReactElement = (
    <NextIntlClientProvider locale="en" messages={messages}>
      <WorkspaceSettingsPage />
    </NextIntlClientProvider>
  );
  render(ui);
}

describe("WorkspaceSettingsPage", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    vi.mocked(ws.getWorkspaceId).mockReset();
    vi.mocked(api.getWorkspace).mockReset();
    vi.mocked(api.updateWorkspace).mockReset();
    vi.mocked(api.updateWorkspaceLocale).mockReset();
    vi.mocked(api.listMembers).mockReset();
    (toast.success as ReturnType<typeof vi.fn>).mockReset();
    (toast.error as ReturnType<typeof vi.fn>).mockReset();
  });

  it("renders the no-workspace placeholder when no workspace id is selected", async () => {
    vi.mocked(ws.getWorkspaceId).mockReturnValue(undefined);
    renderPage();
    await waitFor(() =>
      expect(
        screen.getByText(/No workspace selected/i),
      ).toBeInTheDocument(),
    );
    expect(api.getWorkspace).not.toHaveBeenCalled();
  });

  it("populates the name / slug / locale inputs after load", async () => {
    vi.mocked(ws.getWorkspaceId).mockReturnValue("ws-1");
    vi.mocked(api.getWorkspace).mockResolvedValueOnce(WS);
    vi.mocked(api.listMembers).mockResolvedValueOnce([]);
    renderPage();
    await waitFor(() =>
      expect(screen.getByDisplayValue("Acme analytics")).toBeInTheDocument(),
    );
    // Slug is read-only but still present as a display value.
    expect(screen.getByDisplayValue("acme")).toBeInTheDocument();
    // Primary locale preloaded from the API response.
    expect(screen.getByDisplayValue("ko")).toBeInTheDocument();
    // Fallback joined via comma.
    expect(screen.getByDisplayValue("ko,en")).toBeInTheDocument();
  });

  it("editing the name enables Save and updateWorkspace fires with trimmed value", async () => {
    vi.mocked(ws.getWorkspaceId).mockReturnValue("ws-1");
    vi.mocked(api.getWorkspace).mockResolvedValueOnce(WS);
    vi.mocked(api.listMembers).mockResolvedValueOnce([]);
    vi.mocked(api.updateWorkspace).mockResolvedValueOnce({
      ...WS,
      name: "Acme Labs",
    });
    renderPage();
    await waitFor(() =>
      expect(screen.getByDisplayValue("Acme analytics")).toBeInTheDocument(),
    );
    fireEvent.change(screen.getByDisplayValue("Acme analytics"), {
      target: { value: "  Acme Labs  " },
    });
    // The Save button in the section header is disabled until dirty —
    // after the change it enables. Target the first Save (general block).
    const saveBtns = screen.getAllByRole("button", { name: /^Save$/ });
    fireEvent.click(saveBtns[0]);
    await waitFor(() =>
      expect(api.updateWorkspace).toHaveBeenCalledWith("ws-1", {
        name: "Acme Labs",
      }),
    );
  });

  it("entering an invalid BCP-47 tag keeps the locale Save disabled", async () => {
    vi.mocked(ws.getWorkspaceId).mockReturnValue("ws-1");
    vi.mocked(api.getWorkspace).mockResolvedValueOnce(WS);
    vi.mocked(api.listMembers).mockResolvedValueOnce([]);
    renderPage();
    await waitFor(() =>
      expect(screen.getByDisplayValue("ko")).toBeInTheDocument(),
    );
    // Uppercase ALL-CAPS no-dash tag fails the BCP-47 regex.
    fireEvent.change(screen.getByDisplayValue("ko"), {
      target: { value: "XX-INVALID_" },
    });
    // The locale Save stays disabled because the tag is invalid.
    const localeSave = screen.getByRole("button", { name: /Save locale/ });
    expect(localeSave).toBeDisabled();
    expect(api.updateWorkspaceLocale).not.toHaveBeenCalled();
  });
});
