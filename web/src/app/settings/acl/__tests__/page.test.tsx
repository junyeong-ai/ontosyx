import { describe, it, expect, vi, beforeEach } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { NextIntlClientProvider } from "next-intl";
import type { ReactElement } from "react";

import messages from "../../../../../messages/en.json";

vi.mock("@/lib/api/client", () => ({
  request: vi.fn(),
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

import AclSettingsPage from "@/app/settings/acl/page";
import { request } from "@/lib/api/client";
import { toast } from "sonner";

const SAMPLE_POLICY = {
  id: "policy-1",
  name: "Hide PII from analysts",
  subject_type: "role",
  subject_value: "analyst",
  resource_type: "node_label",
  resource_value: "Customer",
  action: "mask",
  properties: ["email", "phone"],
  mask_pattern: "***",
  priority: 10,
  is_active: true,
};

function renderPage(): void {
  const ui: ReactElement = (
    <NextIntlClientProvider locale="en" messages={messages}>
      <AclSettingsPage />
    </NextIntlClientProvider>
  );
  render(ui);
}

describe("AclSettingsPage", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    (request as ReturnType<typeof vi.fn>).mockReset();
    (toast.success as ReturnType<typeof vi.fn>).mockReset();
    (toast.error as ReturnType<typeof vi.fn>).mockReset();
    confirmMock.mockReset();
  });

  it("lists policies fetched from /acl/policies", async () => {
    (request as ReturnType<typeof vi.fn>).mockResolvedValueOnce([
      SAMPLE_POLICY,
    ]);
    renderPage();
    await waitFor(() =>
      expect(screen.getByText("Hide PII from analysts")).toBeInTheDocument(),
    );
    // Mask pattern is rendered next to the MASK action badge.
    expect(screen.getByText(/\(\*\*\*\)/)).toBeInTheDocument();
    // Properties column joins the list.
    expect(screen.getByText("email, phone")).toBeInTheDocument();
  });

  it("reveals the mask-pattern input only when action is `mask`", async () => {
    (request as ReturnType<typeof vi.fn>).mockResolvedValueOnce([]);
    renderPage();
    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: /Create Policy/ }),
      ).toBeInTheDocument(),
    );
    fireEvent.click(screen.getByRole("button", { name: /Create Policy/ }));
    // Default form action is "deny" → no mask-pattern field.
    expect(
      screen.queryByPlaceholderText(/\*\*\*/),
    ).not.toBeInTheDocument();
    // Switch the action select to "mask" — the field appears.
    // The action select is the third combobox (subject_type /
    // resource_type / action in document order).
    const actionSelect = screen.getAllByRole("combobox")[2];
    fireEvent.change(actionSelect, { target: { value: "mask" } });
    expect(screen.getByPlaceholderText(/\*\*\*/)).toBeInTheDocument();
    // Switch back to "deny" — the field hides again.
    fireEvent.change(actionSelect, { target: { value: "deny" } });
    expect(
      screen.queryByPlaceholderText(/\*\*\*/),
    ).not.toBeInTheDocument();
  });

  it("runs the confirm dialog before DELETE — cancel keeps the row", async () => {
    (request as ReturnType<typeof vi.fn>).mockResolvedValueOnce([
      SAMPLE_POLICY,
    ]);
    confirmMock.mockResolvedValueOnce(false); // user cancels
    renderPage();
    await waitFor(() =>
      expect(screen.getByText(SAMPLE_POLICY.name)).toBeInTheDocument(),
    );
    fireEvent.click(screen.getByRole("button", { name: /^Delete$/ }));
    await waitFor(() => expect(confirmMock).toHaveBeenCalled());
    // No DELETE request — only the initial dashboard GET.
    expect(
      (request as ReturnType<typeof vi.fn>).mock.calls.filter(
        ([, init]) =>
          init &&
          typeof init === "object" &&
          (init as { method?: string }).method === "DELETE",
      ),
    ).toHaveLength(0);
  });

  it("confirm=true issues DELETE and fires success toast", async () => {
    (request as ReturnType<typeof vi.fn>)
      .mockResolvedValueOnce([SAMPLE_POLICY]) // initial load
      .mockResolvedValueOnce(undefined) // DELETE
      .mockResolvedValueOnce([]); // reload
    confirmMock.mockResolvedValueOnce(true);
    renderPage();
    await waitFor(() =>
      expect(screen.getByText(SAMPLE_POLICY.name)).toBeInTheDocument(),
    );
    fireEvent.click(screen.getByRole("button", { name: /^Delete$/ }));
    await waitFor(() => expect(toast.success).toHaveBeenCalled());
    expect(request).toHaveBeenCalledWith(
      `/acl/policies/${SAMPLE_POLICY.id}`,
      expect.objectContaining({ method: "DELETE" }),
    );
  });
});
