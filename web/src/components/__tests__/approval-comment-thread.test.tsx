import { describe, it, expect, vi, beforeEach } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { NextIntlClientProvider } from "next-intl";
import type { ReactElement } from "react";

import messages from "../../../messages/en.json";
import { CommentThread } from "@/components/settings/approvals/comment-thread";
import * as approvalsApi from "@/lib/api/approvals";

vi.mock("sonner", () => ({
  toast: { success: vi.fn(), error: vi.fn() },
}));

function renderWithProviders(ui: ReactElement) {
  return render(
    <NextIntlClientProvider locale="en" messages={messages}>
      {ui}
    </NextIntlClientProvider>,
  );
}

const sample = (overrides: Partial<approvalsApi.ApprovalComment> = {}): approvalsApi.ApprovalComment => ({
  id: "c1",
  workspace_id: "00000000-0000-0000-0000-000000000001",
  approval_id: "appr-1",
  author_id: "abcdef0123456789",
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

  it("posts a new comment and appends it to the thread", async () => {
    vi.spyOn(approvalsApi, "listApprovalComments").mockResolvedValue([]);
    const create = vi
      .spyOn(approvalsApi, "createApprovalComment")
      .mockResolvedValue(sample({ id: "new", body: "Hello" }));

    renderWithProviders(<CommentThread approvalId="appr-1" />);

    // Wait for the loading pass to complete so the textarea + button render.
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
