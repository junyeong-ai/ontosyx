import { describe, it, expect, vi, beforeEach } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { NextIntlClientProvider } from "next-intl";

import messages from "../../../../messages/en.json";

const confirmMock = vi.fn();
vi.mock("@/components/providers/confirm-provider", () => ({
  useConfirm: () => confirmMock,
  ConfirmProvider: ({ children }: { children: React.ReactNode }) => children,
}));

vi.mock("@/lib/workspace", () => ({
  setWorkspaceId: vi.fn(),
}));

vi.mock("@/components/ui/toast", () => ({
  toast: { success: vi.fn(), error: vi.fn() },
}));

vi.mock("@/hooks/api/use-workspaces", () => ({
  useDeleteWorkspace: vi.fn(),
}));

import { WorkspaceDangerZone } from "../workspace-danger-zone";
import { toast } from "@/components/ui/toast";
import * as ws from "@/lib/workspace";
import { useDeleteWorkspace } from "@/hooks/api/use-workspaces";
import type { Workspace } from "@/types/workspace";

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

function renderZone(): void {
  render(
    <NextIntlClientProvider locale="en" messages={messages}>
      <WorkspaceDangerZone workspace={WS} />
    </NextIntlClientProvider>,
  );
}

describe("WorkspaceDangerZone", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    confirmMock.mockReset();
    (toast.success as ReturnType<typeof vi.fn>).mockReset();
    (toast.error as ReturnType<typeof vi.fn>).mockReset();
    vi.mocked(ws.setWorkspaceId).mockReset();
    vi.mocked(useDeleteWorkspace).mockReturnValue({
      mutateAsync: vi.fn().mockResolvedValue(undefined),
      isPending: false,
    } as unknown as ReturnType<typeof useDeleteWorkspace>);
  });

  it("invokes the confirm gate with the slug as the type-to-confirm phrase", async () => {
    confirmMock.mockResolvedValueOnce(true);
    const mutateAsync = vi.fn().mockResolvedValue(undefined);
    vi.mocked(useDeleteWorkspace).mockReturnValue({
      mutateAsync,
      isPending: false,
    } as unknown as ReturnType<typeof useDeleteWorkspace>);
    const assign = vi.fn();
    Object.defineProperty(window, "location", {
      writable: true,
      value: { ...window.location, assign },
    });

    renderZone();
    fireEvent.click(
      screen.getByRole("button", { name: /^Delete workspace$/ }),
    );
    await waitFor(() => expect(confirmMock).toHaveBeenCalledTimes(1));

    const args = confirmMock.mock.calls[0][0];
    expect(args.variant).toBe("danger");
    expect(args.typeToConfirm.phrase).toBe("acme");

    await waitFor(() => expect(mutateAsync).toHaveBeenCalledWith("ws-1"));
    await waitFor(() =>
      expect(toast.success).toHaveBeenCalledWith("Workspace deleted"),
    );
    expect(ws.setWorkspaceId).toHaveBeenCalledWith(undefined);
    expect(assign).toHaveBeenCalledWith("/");
  });

  it("declining the confirm gate is a no-op", async () => {
    confirmMock.mockResolvedValueOnce(false);
    const mutateAsync = vi.fn().mockResolvedValue(undefined);
    vi.mocked(useDeleteWorkspace).mockReturnValue({
      mutateAsync,
      isPending: false,
    } as unknown as ReturnType<typeof useDeleteWorkspace>);

    renderZone();
    fireEvent.click(
      screen.getByRole("button", { name: /^Delete workspace$/ }),
    );
    await waitFor(() => expect(confirmMock).toHaveBeenCalledTimes(1));
    expect(mutateAsync).not.toHaveBeenCalled();
  });

  it("surfaces a toast and stops on mutation failure", async () => {
    confirmMock.mockResolvedValueOnce(true);
    const mutateAsync = vi.fn().mockRejectedValue(new Error("server"));
    vi.mocked(useDeleteWorkspace).mockReturnValue({
      mutateAsync,
      isPending: false,
    } as unknown as ReturnType<typeof useDeleteWorkspace>);

    renderZone();
    fireEvent.click(
      screen.getByRole("button", { name: /^Delete workspace$/ }),
    );
    await waitFor(() =>
      expect(toast.error).toHaveBeenCalledWith(
        "Failed to delete workspace",
      ),
    );
    expect(toast.success).not.toHaveBeenCalled();
    expect(ws.setWorkspaceId).not.toHaveBeenCalled();
  });
});
