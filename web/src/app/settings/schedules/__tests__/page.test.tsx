import { describe, it, expect, vi, beforeEach } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { NextIntlClientProvider } from "next-intl";
import type { ReactElement } from "react";

import messages from "../../../../../messages/en.json";

// Admin API re-exports via `@/lib/api`; mock the three named
// exports the page imports directly.
vi.mock("@/lib/api", async () => {
  const actual = await vi.importActual<Record<string, unknown>>("@/lib/api");
  return {
    ...actual,
    listScheduledTasks: vi.fn(),
    updateScheduledTask: vi.fn(),
    deleteScheduledTask: vi.fn(),
  };
});

const confirmMock = vi.fn();
vi.mock("@/components/ui/confirm-dialog", () => ({
  useConfirm: () => confirmMock,
  ConfirmDialogProvider: ({ children }: { children: React.ReactNode }) =>
    children,
}));

vi.mock("sonner", () => ({
  toast: { success: vi.fn(), error: vi.fn() },
}));

import SchedulesPage from "@/app/settings/schedules/page";
import {
  listScheduledTasks,
  updateScheduledTask,
  deleteScheduledTask,
} from "@/lib/api";
import { toast } from "sonner";

const SAMPLE_TASK = {
  id: "task-1",
  description: "Nightly stale concept scan",
  cron_expression: "0 2 * * *",
  enabled: true,
  last_run_at: "2026-04-22T02:00:00Z",
  next_run_at: "2026-04-24T02:00:00Z",
  last_status: "completed",
  last_error: null,
  workspace_id: "ws-1",
  created_at: "2026-04-22T00:00:00Z",
  updated_at: "2026-04-23T02:00:00Z",
} as unknown as Awaited<ReturnType<typeof listScheduledTasks>>[number];

function renderPage(): void {
  const ui: ReactElement = (
    <NextIntlClientProvider locale="en" messages={messages}>
      <SchedulesPage />
    </NextIntlClientProvider>
  );
  render(ui);
}

describe("SchedulesPage", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    vi.mocked(listScheduledTasks).mockReset();
    vi.mocked(updateScheduledTask).mockReset();
    vi.mocked(deleteScheduledTask).mockReset();
    (toast.success as ReturnType<typeof vi.fn>).mockReset();
    (toast.error as ReturnType<typeof vi.fn>).mockReset();
    confirmMock.mockReset();
  });

  it("renders the task row once `listScheduledTasks` resolves", async () => {
    vi.mocked(listScheduledTasks).mockResolvedValueOnce([SAMPLE_TASK]);
    renderPage();
    await waitFor(() =>
      expect(
        screen.getByText("Nightly stale concept scan"),
      ).toBeInTheDocument(),
    );
    expect(screen.getByText("0 2 * * *")).toBeInTheDocument();
  });

  it("renders the empty-state copy when no tasks are returned", async () => {
    vi.mocked(listScheduledTasks).mockResolvedValueOnce([]);
    renderPage();
    // The `empty` i18n string is the user-facing "no scheduled
    // tasks" banner.
    await waitFor(() =>
      expect(
        screen.getByText(/No scheduled tasks|예정된/i),
      ).toBeInTheDocument(),
    );
  });

  it("delete with confirm=false leaves the row alone and fires no API call", async () => {
    vi.mocked(listScheduledTasks).mockResolvedValueOnce([SAMPLE_TASK]);
    confirmMock.mockResolvedValueOnce(false);
    renderPage();
    await waitFor(() =>
      expect(screen.getByText(SAMPLE_TASK.description)).toBeInTheDocument(),
    );
    // The delete button renders with `common.delete` → "Delete".
    fireEvent.click(screen.getByRole("button", { name: /^Delete$/ }));
    await waitFor(() => expect(confirmMock).toHaveBeenCalled());
    expect(deleteScheduledTask).not.toHaveBeenCalled();
    // Row stays.
    expect(screen.getByText(SAMPLE_TASK.description)).toBeInTheDocument();
  });

  it("delete with confirm=true reverts optimistic remove if the API fails", async () => {
    vi.mocked(listScheduledTasks).mockResolvedValueOnce([SAMPLE_TASK]);
    confirmMock.mockResolvedValueOnce(true);
    vi.mocked(deleteScheduledTask).mockRejectedValueOnce(new Error("boom"));
    renderPage();
    await waitFor(() =>
      expect(screen.getByText(SAMPLE_TASK.description)).toBeInTheDocument(),
    );
    fireEvent.click(screen.getByRole("button", { name: /^Delete$/ }));
    // After the failure, the row should reappear (snapshot
    // revert) and the error toast should fire.
    await waitFor(() => expect(toast.error).toHaveBeenCalled());
    expect(screen.getByText(SAMPLE_TASK.description)).toBeInTheDocument();
  });
});
