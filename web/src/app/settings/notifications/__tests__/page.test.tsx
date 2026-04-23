import { describe, it, expect, vi, beforeEach } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { NextIntlClientProvider } from "next-intl";
import type { ReactElement } from "react";

import messages from "../../../../../messages/en.json";

// Each notifications API call gets its own spy — the page pulls
// listChannels() + listLogs() in parallel via Promise.all at mount.
vi.mock("@/lib/api/notifications", () => ({
  listChannels: vi.fn(),
  createChannel: vi.fn(),
  updateChannel: vi.fn(),
  deleteChannel: vi.fn(),
  testChannel: vi.fn(),
  listLogs: vi.fn(),
}));

const confirmMock = vi.fn();
vi.mock("@/components/ui/confirm-dialog", () => ({
  useConfirm: () => confirmMock,
  ConfirmDialogProvider: ({ children }: { children: React.ReactNode }) =>
    children,
}));

vi.mock("sonner", () => ({
  toast: { success: vi.fn(), error: vi.fn() },
}));

import NotificationsSettingsPage from "@/app/settings/notifications/page";
import * as notificationsApi from "@/lib/api/notifications";
import { toast } from "sonner";

type Channel = Awaited<
  ReturnType<typeof notificationsApi.listChannels>
>[number];

const SAMPLE_CHANNEL: Channel = {
  id: "ch-1",
  name: "Alerts channel",
  channel_type: "slack_webhook",
  config: { url: "https://hooks.slack.com/T000/B000/XXX" },
  events: ["quality_rule_failed"],
  enabled: true,
  created_at: "2026-04-23T00:00:00Z",
  updated_at: "2026-04-23T00:00:00Z",
} as unknown as Channel;

function renderPage(): void {
  const ui: ReactElement = (
    <NextIntlClientProvider locale="en" messages={messages}>
      <NotificationsSettingsPage />
    </NextIntlClientProvider>
  );
  render(ui);
}

describe("NotificationsSettingsPage", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    vi.mocked(notificationsApi.listChannels).mockReset();
    vi.mocked(notificationsApi.listLogs).mockReset();
    vi.mocked(notificationsApi.createChannel).mockReset();
    vi.mocked(notificationsApi.updateChannel).mockReset();
    vi.mocked(notificationsApi.deleteChannel).mockReset();
    vi.mocked(notificationsApi.testChannel).mockReset();
    (toast.success as ReturnType<typeof vi.fn>).mockReset();
    (toast.error as ReturnType<typeof vi.fn>).mockReset();
    confirmMock.mockReset();
  });

  it("fetches channels + logs in parallel and renders both sections", async () => {
    vi.mocked(notificationsApi.listChannels).mockResolvedValueOnce([
      SAMPLE_CHANNEL,
    ]);
    vi.mocked(notificationsApi.listLogs).mockResolvedValueOnce([]);
    renderPage();
    await waitFor(() =>
      expect(screen.getByText("Alerts channel")).toBeInTheDocument(),
    );
    expect(notificationsApi.listChannels).toHaveBeenCalledTimes(1);
    expect(notificationsApi.listLogs).toHaveBeenCalledWith(50);
    // Both section headings are rendered — channels always (has
    // row), logs heading is always rendered even when empty.
    expect(screen.getByText("Channels")).toBeInTheDocument();
    expect(screen.getByText("Recent Notifications")).toBeInTheDocument();
  });

  it("create submit with a non-URL string does NOT call createChannel", async () => {
    vi.mocked(notificationsApi.listChannels).mockResolvedValueOnce([]);
    vi.mocked(notificationsApi.listLogs).mockResolvedValueOnce([]);
    renderPage();
    await waitFor(() =>
      expect(
        screen.getAllByRole("button", { name: /Add Channel/ })[0],
      ).toBeInTheDocument(),
    );
    fireEvent.click(
      screen.getAllByRole("button", { name: /Add Channel/ })[0],
    );
    fireEvent.change(screen.getByPlaceholderText(/Slack #alerts/), {
      target: { value: "My hook" },
    });
    fireEvent.change(
      screen.getByPlaceholderText(/hooks\.slack\.com\/services/),
      { target: { value: "not-a-url" } },
    );
    // Click the submit button inside the form. There are two
    // "Create Channel" matches (header CTA + submit), pick the
    // last one (the submit button inside the opened form).
    const submits = screen.getAllByRole("button", {
      name: /^Create Channel$/,
    });
    fireEvent.click(submits[submits.length - 1]);
    // createChannel should never have fired — validation rejected
    // the URL.
    await waitFor(() =>
      expect(notificationsApi.createChannel).not.toHaveBeenCalled(),
    );
    expect(screen.getByText("Invalid URL")).toBeInTheDocument();
  });

  it("submitting with zero events selected surfaces `selectEvent` error", async () => {
    vi.mocked(notificationsApi.listChannels).mockResolvedValueOnce([]);
    vi.mocked(notificationsApi.listLogs).mockResolvedValueOnce([]);
    renderPage();
    await waitFor(() =>
      expect(
        screen.getAllByRole("button", { name: /Add Channel/ })[0],
      ).toBeInTheDocument(),
    );
    fireEvent.click(
      screen.getAllByRole("button", { name: /Add Channel/ })[0],
    );
    // Untick the default-selected quality_rule_failed event —
    // the form now has zero events.
    const failedCheckbox = screen.getByLabelText(/Quality Rule Failed/);
    fireEvent.click(failedCheckbox);
    // Fill valid name + URL so the other validations pass.
    fireEvent.change(screen.getByPlaceholderText(/Slack #alerts/), {
      target: { value: "Valid" },
    });
    fireEvent.change(
      screen.getByPlaceholderText(/hooks\.slack\.com\/services/),
      { target: { value: "https://hooks.slack.com/services/T/B/X" } },
    );
    const submits = screen.getAllByRole("button", {
      name: /^Create Channel$/,
    });
    fireEvent.click(submits[submits.length - 1]);
    await waitFor(() =>
      expect(screen.getByText("Select at least one event")).toBeInTheDocument(),
    );
    expect(notificationsApi.createChannel).not.toHaveBeenCalled();
  });

  it("test button calls testChannel and surfaces success toast", async () => {
    vi.mocked(notificationsApi.listChannels).mockResolvedValueOnce([
      SAMPLE_CHANNEL,
    ]);
    vi.mocked(notificationsApi.listLogs).mockResolvedValueOnce([]);
    vi.mocked(notificationsApi.testChannel).mockResolvedValueOnce({
      success: true,
    });
    // Reload after test fires.
    vi.mocked(notificationsApi.listChannels).mockResolvedValueOnce([
      SAMPLE_CHANNEL,
    ]);
    vi.mocked(notificationsApi.listLogs).mockResolvedValueOnce([]);
    renderPage();
    await waitFor(() =>
      expect(screen.getByText(SAMPLE_CHANNEL.name)).toBeInTheDocument(),
    );
    fireEvent.click(screen.getByRole("button", { name: /^Test$/ }));
    await waitFor(() =>
      expect(notificationsApi.testChannel).toHaveBeenCalledWith(
        SAMPLE_CHANNEL.id,
      ),
    );
    expect(toast.success).toHaveBeenCalled();
  });
});
