import { describe, it, expect, vi, beforeEach } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { NextIntlClientProvider } from "next-intl";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { ReactElement } from "react";

import messages from "../../../../../../messages/en.json";

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

vi.mock("@/hooks/use-auth", () => ({
  useAuth: vi.fn(),
}));

// The Danger zone is exercised in its own focused test file
// (`workspace-danger-zone.test.tsx`); stubbing it here keeps the
// page test scoped to identity + locale fields.
vi.mock("@/components/settings/workspace-danger-zone", () => ({
  WorkspaceDangerZone: ({
    workspace,
  }: {
    workspace: { id: string; slug: string };
  }) => (
    <div
      data-testid="workspace-danger-zone"
      data-workspace-id={workspace.id}
    />
  ),
}));

// MembersTable lives in a sibling module; keep the test focused on
// workspace identity + locale fields only.
vi.mock("@/components/workspace/members-table", () => ({
  MembersTable: () => <div data-testid="members-table" />,
}));

vi.mock("@/components/ui/toast", () => ({
  toast: { success: vi.fn(), error: vi.fn() },
}));

import WorkspaceSettingsPage from "@/app/settings/workspace/general/page";
import * as api from "@/lib/api/workspaces";
import * as ws from "@/lib/workspace";
import type { Workspace } from "@/types/workspace";
import { toast } from "@/components/ui/toast";
import { useAuth } from "@/hooks/use-auth";

const WS: Workspace = {
  id: "ws-1",
  name: "Acme analytics",
  slug: "acme",
  owner_id: "u-a",
  settings: {},
  primary_locale: "ko",
  admin_locale_fallback: ["ko", "en"],
  llm_locale_fallback: ["en", "ko"],
  created_at: "2026-04-22T00:00:00Z",
};

function renderPage(): void {
  const qc = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  const ui: ReactElement = (
    <QueryClientProvider client={qc}>
      <NextIntlClientProvider locale="en" messages={messages}>
        <WorkspaceSettingsPage />
      </NextIntlClientProvider>
    </QueryClientProvider>
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
    // Default to non-admin so the page renders without the Danger
    // zone; the admin-gating test opts into the admin shape.
    vi.mocked(useAuth).mockReturnValue({
      isAdmin: false,
      canWrite: true,
      user: null,
    } as unknown as ReturnType<typeof useAuth>);
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

  it("hides the Danger zone for non-admin viewers", async () => {
    vi.mocked(ws.getWorkspaceId).mockReturnValue("ws-1");
    vi.mocked(api.getWorkspace).mockResolvedValueOnce(WS);
    vi.mocked(api.listMembers).mockResolvedValueOnce([]);
    renderPage();
    await waitFor(() =>
      expect(screen.getByDisplayValue("Acme analytics")).toBeInTheDocument(),
    );
    expect(
      screen.queryByTestId("workspace-danger-zone"),
    ).not.toBeInTheDocument();
  });

  it("renders the Danger zone for admin viewers and threads the workspace through", async () => {
    vi.mocked(useAuth).mockReturnValue({
      isAdmin: true,
      canWrite: true,
      user: null,
    } as unknown as ReturnType<typeof useAuth>);
    vi.mocked(ws.getWorkspaceId).mockReturnValue("ws-1");
    vi.mocked(api.getWorkspace).mockResolvedValueOnce(WS);
    vi.mocked(api.listMembers).mockResolvedValueOnce([]);
    renderPage();
    await waitFor(() =>
      expect(screen.getByTestId("workspace-danger-zone")).toBeInTheDocument(),
    );
    expect(
      screen.getByTestId("workspace-danger-zone").dataset.workspaceId,
    ).toBe("ws-1");
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
