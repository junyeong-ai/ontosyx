import { describe, it, expect, vi, beforeEach } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { NextIntlClientProvider } from "next-intl";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { ReactElement } from "react";

import messages from "../../../../../../messages/en.json";

vi.mock("@/hooks/use-auth", () => ({
  useAuth: vi.fn(),
}));

vi.mock("@/lib/api", () => ({
  listFederationAdapters: vi.fn(),
  registerFederationAdapter: vi.fn(),
  deleteFederationAdapter: vi.fn(),
  refreshFederationAdapters: vi.fn(),
  getFederationHealth: vi.fn(),
  previewFederationAdapter: vi.fn(),
}));

const confirmMock = vi.fn();
vi.mock("@/components/providers/confirm-provider", () => ({
  useConfirm: () => confirmMock,
  ConfirmDialogProvider: ({ children }: { children: React.ReactNode }) =>
    children,
}));

vi.mock("@/components/ui/toast", () => ({
  toast: { success: vi.fn(), error: vi.fn() },
}));

import FederationAdaptersPage from "@/app/settings/knowledge/federation/page";
import * as api from "@/lib/api";
import { useAuth } from "@/hooks/use-auth";
import { mockAuth } from "@/test-utils/auth";
import { toast } from "@/components/ui/toast";

const SAMPLE_ADAPTER = {
  source_id: "csv-orders",
  source_type: "csv",
  supports_scan: true,
};

const HEALTH_OK = {
  workspace_id: "ws-1",
  resolver_hydrated: true,
  resolver_count: 1,
  store_count: 1,
  in_sync: true,
  orphans_in_resolver: [],
  missing_from_resolver: [],
};

function renderPage(): void {
  const qc = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  const ui: ReactElement = (
    <NextIntlClientProvider locale="en" messages={messages}>
      <QueryClientProvider client={qc}>
        <FederationAdaptersPage />
      </QueryClientProvider>
    </NextIntlClientProvider>
  );
  render(ui);
}

function asAdmin(): void {
  vi.mocked(useAuth).mockReturnValue(
    mockAuth(
      { kind: "authenticated", role: "admin" },
      { sub: "u1", email: "a@b.c", name: "Admin" },
    ),
  );
}

describe("FederationAdaptersPage", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    vi.mocked(api.listFederationAdapters).mockReset();
    vi.mocked(api.getFederationHealth).mockReset();
    vi.mocked(api.registerFederationAdapter).mockReset();
    vi.mocked(api.deleteFederationAdapter).mockReset();
    vi.mocked(api.previewFederationAdapter).mockReset();
    (toast.success as ReturnType<typeof vi.fn>).mockReset();
    (toast.error as ReturnType<typeof vi.fn>).mockReset();
    confirmMock.mockReset();
  });

  it("renders the admin-only notice when the viewer is not an admin", async () => {
    vi.mocked(useAuth).mockReturnValue(
      mockAuth({ kind: "authenticated", role: "viewer" }),
    );
    renderPage();
    await waitFor(() =>
      expect(
        screen.getByText(
          "Federation adapter management requires admin privileges.",
        ),
      ).toBeInTheDocument(),
    );
    // No adapter list fetched — the gate shortcircuits.
    expect(api.listFederationAdapters).not.toHaveBeenCalled();
  });

  it("fetches adapters + health in parallel and renders both sections", async () => {
    asAdmin();
    vi.mocked(api.listFederationAdapters).mockResolvedValueOnce([
      SAMPLE_ADAPTER,
    ]);
    vi.mocked(api.getFederationHealth).mockResolvedValueOnce(HEALTH_OK);
    renderPage();
    await waitFor(() =>
      expect(screen.getByText("csv-orders")).toBeInTheDocument(),
    );
    expect(api.listFederationAdapters).toHaveBeenCalledTimes(1);
    expect(api.getFederationHealth).toHaveBeenCalledTimes(1);
    // Health card shows the in-sync marker when the API reports no drift.
    expect(screen.getByText("In sync")).toBeInTheDocument();
  });

  it("submitting with an empty source_id does NOT call the register API", async () => {
    asAdmin();
    vi.mocked(api.listFederationAdapters).mockResolvedValueOnce([]);
    vi.mocked(api.getFederationHealth).mockResolvedValueOnce(HEALTH_OK);
    renderPage();
    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: /^Register$/ }),
      ).toBeInTheDocument(),
    );
    // The submit button is disabled on empty source_id — clicking it
    // should be a no-op and register must stay untouched.
    fireEvent.click(screen.getByRole("button", { name: /^Register$/ }));
    await waitFor(() => {
      expect(api.registerFederationAdapter).not.toHaveBeenCalled();
    });
  });

  it("preview button calls previewFederationAdapter and renders the schema panel", async () => {
    asAdmin();
    vi.mocked(api.listFederationAdapters).mockResolvedValueOnce([]);
    vi.mocked(api.getFederationHealth).mockResolvedValueOnce(HEALTH_OK);
    vi.mocked(api.previewFederationAdapter).mockResolvedValueOnce({
      source_type: "csv",
      tables: [
        {
          name: "records",
          columns: [{ name: "id", data_type: "string", nullable: false }],
        },
      ],
    });
    renderPage();
    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: /Preview schema/ }),
      ).toBeInTheDocument(),
    );
    // Fill the inline CSV payload (kind defaults to csv).
    fireEvent.change(
      screen.getByPlaceholderText(/Inline CSV\/JSON payload/),
      { target: { value: "id,name\n1,a" } },
    );
    fireEvent.click(screen.getByRole("button", { name: /Preview schema/ }));
    await waitFor(() =>
      expect(api.previewFederationAdapter).toHaveBeenCalledTimes(1),
    );
    // The preview panel renders the table + column.
    await waitFor(() =>
      expect(screen.getByText("records")).toBeInTheDocument(),
    );
    expect(screen.getByText("id")).toBeInTheDocument();
  });

  it("confirm=true on delete fires deleteFederationAdapter + reload", async () => {
    asAdmin();
    vi.mocked(api.listFederationAdapters).mockResolvedValueOnce([
      SAMPLE_ADAPTER,
    ]);
    vi.mocked(api.getFederationHealth).mockResolvedValueOnce(HEALTH_OK);
    vi.mocked(api.deleteFederationAdapter).mockResolvedValueOnce(undefined);
    // After delete fires, the page reloads — provide empty responses.
    vi.mocked(api.listFederationAdapters).mockResolvedValueOnce([]);
    vi.mocked(api.getFederationHealth).mockResolvedValueOnce(HEALTH_OK);
    confirmMock.mockResolvedValueOnce(true);
    renderPage();
    await waitFor(() =>
      expect(screen.getByText("csv-orders")).toBeInTheDocument(),
    );
    // The Delete button is in the row actions column.
    fireEvent.click(screen.getByRole("button", { name: /^Delete$/ }));
    await waitFor(() =>
      expect(api.deleteFederationAdapter).toHaveBeenCalledWith("csv-orders"),
    );
    expect(toast.success).toHaveBeenCalled();
  });
});
