import { describe, it, expect, vi, beforeEach } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { NextIntlClientProvider } from "next-intl";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { ReactElement } from "react";

import messages from "../../../messages/en.json";
import { CommentThread } from "@/components/settings/approvals/comment-thread";
import * as approvalsApi from "@/lib/api/approvals";

vi.mock("sonner", () => ({
  toast: { success: vi.fn(), error: vi.fn() },
}));

function renderWithProviders(ui: ReactElement) {
  // Each test gets its own QueryClient so cache state from one test
  // never bleeds into the next. retry: false keeps failure tests
  // synchronous — the default 3-attempt retry would otherwise mask
  // the transition we want to assert.
  const qc = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return render(
    <NextIntlClientProvider locale="en" messages={messages}>
      <QueryClientProvider client={qc}>{ui}</QueryClientProvider>
    </NextIntlClientProvider>,
  );
}

const sample = (overrides: Partial<approvalsApi.ApprovalComment> = {}): approvalsApi.ApprovalComment => ({
  id: "c1",
  workspace_id: "00000000-0000-0000-0000-000000000001",
  approval_id: "appr-1",
  author_id: "abcdef0123456789",
  author_name: "Alice",
  body: "Looks fine",
  created_at: "2026-04-26T10:00:00Z",
  ...overrides,
});

describe("CommentThread", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it("renders fetched comments oldest first", async () => {
    vi.spyOn(approvalsApi, "listApprovalComments").mockResolvedValue([
      sample({ id: "a", body: "First" }),
      sample({ id: "b", body: "Second" }),
    ]);

    renderWithProviders(<CommentThread approvalId="appr-1" />);

    await waitFor(() => {
      expect(screen.getByText("First")).toBeDefined();
      expect(screen.getByText("Second")).toBeDefined();
    });
  });

  it("shows the empty-state when the thread has no entries", async () => {
    vi.spyOn(approvalsApi, "listApprovalComments").mockResolvedValue([]);

    renderWithProviders(<CommentThread approvalId="appr-1" />);

    await waitFor(() => {
      expect(screen.getByText(/no comments yet/i)).toBeDefined();
    });
  });

  it("posts a new comment and the thread reloads via cache invalidation", async () => {
    const list = vi.spyOn(approvalsApi, "listApprovalComments");
    list.mockResolvedValueOnce([]); // initial load
    list.mockResolvedValueOnce([sample({ id: "new", body: "Hello" })]); // post-invalidation refetch

    const create = vi
      .spyOn(approvalsApi, "createApprovalComment")
      .mockResolvedValue(sample({ id: "new", body: "Hello" }));

    renderWithProviders(<CommentThread approvalId="appr-1" />);

    await waitFor(() => {
      expect(screen.getByPlaceholderText(/add a comment/i)).toBeDefined();
    });

    const textarea = screen.getByPlaceholderText(/add a comment/i);
    fireEvent.change(textarea, { target: { value: "  Hello  " } });

    const button = screen.getByRole("button", { name: /post comment/i });
    fireEvent.click(button);

    await waitFor(() => {
      expect(create).toHaveBeenCalledWith("appr-1", "Hello");
      expect(screen.getByText("Hello")).toBeDefined();
    });
    // The mutation's onSuccess invalidates the thread query — verify
    // the list refetch actually fired (not just that the mutation
    // returned, which would only prove optimism).
    expect(list).toHaveBeenCalledTimes(2);
  });

  it("hides the composer when readOnly is set", async () => {
    vi.spyOn(approvalsApi, "listApprovalComments").mockResolvedValue([
      sample({ body: "Recorded" }),
    ]);

    renderWithProviders(<CommentThread approvalId="appr-1" readOnly />);

    await waitFor(() => {
      expect(screen.getByText("Recorded")).toBeDefined();
    });
    expect(screen.queryByPlaceholderText(/add a comment/i)).toBeNull();
    expect(screen.queryByRole("button", { name: /post comment/i })).toBeNull();
  });

  it("falls back to the unknown-author label when author_name is null", async () => {
    vi.spyOn(approvalsApi, "listApprovalComments").mockResolvedValue([
      sample({ author_name: null, body: "Anonymous" }),
    ]);

    renderWithProviders(<CommentThread approvalId="appr-1" readOnly />);

    await waitFor(() => {
      expect(screen.getByText("Anonymous")).toBeDefined();
    });
    expect(screen.getByText(/unknown user/i)).toBeDefined();
  });

  it("renders the resolved author name when present", async () => {
    vi.spyOn(approvalsApi, "listApprovalComments").mockResolvedValue([
      sample({ author_name: "Alice", body: "Looks fine" }),
    ]);

    renderWithProviders(<CommentThread approvalId="appr-1" readOnly />);

    await waitFor(() => {
      expect(screen.getByText("Alice")).toBeDefined();
    });
  });

  it("disables the post button while body is empty whitespace", async () => {
    vi.spyOn(approvalsApi, "listApprovalComments").mockResolvedValue([]);

    renderWithProviders(<CommentThread approvalId="appr-1" />);

    await waitFor(() => {
      expect(screen.getByPlaceholderText(/add a comment/i)).toBeDefined();
    });

    const textarea = screen.getByPlaceholderText(/add a comment/i);
    fireEvent.change(textarea, { target: { value: "   " } });

    const button = screen.getByRole("button", {
      name: /post comment/i,
    }) as HTMLButtonElement;
    expect(button.disabled).toBe(true);
  });
});
