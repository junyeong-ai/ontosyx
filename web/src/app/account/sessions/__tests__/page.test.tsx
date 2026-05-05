import { describe, it, expect, vi, beforeEach } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { NextIntlClientProvider } from "next-intl";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { ReactElement } from "react";

import messages from "../../../../../messages/en.json";

vi.mock("next/navigation", () => ({
  useRouter: () => ({
    push: vi.fn(),
    replace: vi.fn(),
    back: vi.fn(),
    forward: vi.fn(),
    refresh: vi.fn(),
    prefetch: vi.fn(),
  }),
  usePathname: () => "/settings/sessions",
  useSearchParams: () => new URLSearchParams(""),
}));

vi.mock("@/lib/api", async () => {
  const actual = await vi.importActual<Record<string, unknown>>("@/lib/api");
  return {
    ...actual,
    listAgentSessions: vi.fn(),
    listAgentEvents: vi.fn(),
    fetchSessionMessages: vi.fn(),
    deleteSession: vi.fn(),
  };
});

const confirmMock = vi.fn();
vi.mock("@/components/providers/confirm-provider", () => ({
  useConfirm: () => confirmMock,
  ConfirmDialogProvider: ({ children }: { children: React.ReactNode }) =>
    children,
}));

vi.mock("@/components/ui/toast", () => ({
  toast: { success: vi.fn(), error: vi.fn() },
}));

import SessionsPage from "@/app/account/sessions/page";
import {
  listAgentSessions,
  deleteSession,
  fetchSessionMessages,
} from "@/lib/api";
import { toast } from "@/components/ui/toast";

function makeSession(overrides: Partial<Record<string, unknown>> = {}) {
  return {
    id: "s-1",
    user_message: "find duplicates",
    model_id: "anthropic/claude-opus-4-7",
    prompt_hash: "0123456789abcdef0123456789abcdef",
    tool_schema_hash: "fedcba9876543210fedcba9876543210",
    created_at: "2026-04-22T00:00:00Z",
    completed_at: "2026-04-22T00:01:00Z",
    ...overrides,
  } as unknown as Awaited<ReturnType<typeof listAgentSessions>>["items"][number];
}

function renderPage(): void {
  const qc = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  const ui: ReactElement = (
    <NextIntlClientProvider locale="en" messages={messages}>
      <QueryClientProvider client={qc}>
        <SessionsPage />
      </QueryClientProvider>
    </NextIntlClientProvider>
  );
  render(ui);
}

describe("SessionsPage", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    vi.mocked(listAgentSessions).mockReset();
    vi.mocked(deleteSession).mockReset();
    vi.mocked(fetchSessionMessages).mockReset();
    (toast.success as ReturnType<typeof vi.fn>).mockReset();
    (toast.error as ReturnType<typeof vi.fn>).mockReset();
    confirmMock.mockReset();
  });

  it("aggregates the four StatCard counts from the session list", async () => {
    const recent = makeSession({
      id: "s-recent",
      created_at: new Date().toISOString(),
      completed_at: null,
    });
    const oldCompleted = makeSession({
      id: "s-old",
      created_at: "2024-01-01T00:00:00Z",
      completed_at: "2024-01-01T00:01:00Z",
      model_id: "openai/gpt-4o",
    });
    vi.mocked(listAgentSessions).mockResolvedValueOnce({
      items: [recent, oldCompleted],
      next_cursor: undefined,
    } as Awaited<ReturnType<typeof listAgentSessions>>);

    renderPage();
    // Total = 2 (after loading state clears)
    await waitFor(() =>
      expect(screen.getByText("Total Sessions")).toBeInTheDocument(),
    );
    // Two distinct model_ids → modelsUsed = 2.
    // Total + Models Used both render "2" — assert at least one.
    const twos = screen.getAllByText("2");
    expect(twos.length).toBeGreaterThanOrEqual(2);
    // 1 completed (the old session).
    expect(screen.getAllByText("1").length).toBeGreaterThanOrEqual(1);
  });

  it("filters the session list by the search input substring", async () => {
    vi.mocked(listAgentSessions).mockResolvedValueOnce({
      items: [
        makeSession({ id: "a", user_message: "alpha message" }),
        makeSession({ id: "b", user_message: "beta message" }),
      ],
      next_cursor: undefined,
    } as Awaited<ReturnType<typeof listAgentSessions>>);
    renderPage();

    await waitFor(() =>
      expect(screen.getByText("alpha message")).toBeInTheDocument(),
    );
    expect(screen.getByText("beta message")).toBeInTheDocument();

    fireEvent.change(screen.getByPlaceholderText(/Search by message/), {
      target: { value: "alpha" },
    });
    await waitFor(() =>
      expect(screen.queryByText("beta message")).not.toBeInTheDocument(),
    );
    expect(screen.getByText("alpha message")).toBeInTheDocument();
  });

  it("delete with confirm=false skips the API and keeps the row", async () => {
    vi.mocked(listAgentSessions).mockResolvedValueOnce({
      items: [makeSession({ user_message: "keep me" })],
      next_cursor: undefined,
    } as Awaited<ReturnType<typeof listAgentSessions>>);
    confirmMock.mockResolvedValueOnce(false);
    renderPage();

    await waitFor(() => expect(screen.getByText("keep me")).toBeInTheDocument());
    // The delete affordance is hidden until hover but still in the DOM.
    fireEvent.click(screen.getByRole("button", { name: /^Delete$/ }));
    await waitFor(() => expect(confirmMock).toHaveBeenCalled());
    expect(deleteSession).not.toHaveBeenCalled();
    expect(screen.getByText("keep me")).toBeInTheDocument();
  });

  it("delete with confirm=true removes the row + fires success toast", async () => {
    vi.mocked(listAgentSessions).mockResolvedValueOnce({
      items: [makeSession({ id: "to-go", user_message: "remove me" })],
      next_cursor: undefined,
    } as Awaited<ReturnType<typeof listAgentSessions>>);
    confirmMock.mockResolvedValueOnce(true);
    vi.mocked(deleteSession).mockResolvedValueOnce(undefined);
    renderPage();

    await waitFor(() =>
      expect(screen.getByText("remove me")).toBeInTheDocument(),
    );
    fireEvent.click(screen.getByRole("button", { name: /^Delete$/ }));
    await waitFor(() => expect(deleteSession).toHaveBeenCalledWith("to-go"));
    await waitFor(() =>
      expect(toast.success).toHaveBeenCalledWith("Session deleted"),
    );
    expect(screen.queryByText("remove me")).not.toBeInTheDocument();
  });
});
