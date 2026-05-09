import { describe, it, expect, vi, beforeEach } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { NextIntlClientProvider } from "next-intl";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { ReactElement } from "react";

import messages from "../../../../../../messages/en.json";

vi.mock("@/lib/api/client", () => ({
  request: vi.fn(),
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

import ModelsSettingsPage from "@/app/settings/runtime/models/page";
import { request } from "@/lib/api/client";
import { toast } from "@/components/ui/toast";

const CONFIG_ENABLED = {
  id: "cfg-1",
  name: "Opus prod",
  provider: "anthropic",
  model_id: "claude-opus-4-7",
  max_tokens: 4096,
  temperature: 0.2,
  timeout_secs: 120,
  cost_per_1m_input: 15,
  cost_per_1m_output: 75,
  daily_budget_usd: 100,
  priority: 0,
  enabled: true,
  api_key_env: "ANTHROPIC_API_KEY",
  region: null,
  base_url: null,
  provider_meta: {},
};

const CONFIG_DISABLED = {
  ...CONFIG_ENABLED,
  id: "cfg-2",
  name: "Sonnet backup",
  model_id: "claude-sonnet-4-6",
  enabled: false,
};

const RULE = {
  id: "rule-1",
  operation: "design_ontology",
  model_config_id: "cfg-1",
  priority: 100,
  enabled: true,
};

function setMocks(
  configs: typeof CONFIG_ENABLED[] = [],
  rules: typeof RULE[] = [],
) {
  vi.mocked(request).mockImplementation((url: string) => {
    if (url === "/models/operations") return Promise.resolve([]);
    if (url === "/models/configs") return Promise.resolve(configs);
    if (url === "/models/routing-rules") return Promise.resolve(rules);
    return Promise.resolve(undefined);
  });
}

describe("ModelsSettingsPage", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    vi.mocked(request).mockReset();
    (toast.success as ReturnType<typeof vi.fn>).mockReset();
    (toast.error as ReturnType<typeof vi.fn>).mockReset();
    confirmMock.mockReset();
  });

  it("renders both config rows + the routing rule once initial data resolves", async () => {
    setMocks([CONFIG_ENABLED, CONFIG_DISABLED], [RULE]);
    const qc = new QueryClient({
      defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
    });
    const ui: ReactElement = (
      <NextIntlClientProvider locale="en" messages={messages}>
        <QueryClientProvider client={qc}>
          <ModelsSettingsPage />
        </QueryClientProvider>
      </NextIntlClientProvider>
    );
    render(ui);
    await waitFor(() =>
      expect(screen.getAllByText("Opus prod").length).toBeGreaterThan(0),
    );
    expect(screen.getByText("Sonnet backup")).toBeInTheDocument();
    // Rule row uses the human config name (not the uuid).
    expect(screen.getByText("design_ontology")).toBeInTheDocument();
  });

  it("computes the three summary cards from the config + rule lists", async () => {
    setMocks([CONFIG_ENABLED, CONFIG_DISABLED], [RULE]);
    const qc = new QueryClient({
      defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
    });
    const ui: ReactElement = (
      <NextIntlClientProvider locale="en" messages={messages}>
        <QueryClientProvider client={qc}>
          <ModelsSettingsPage />
        </QueryClientProvider>
      </NextIntlClientProvider>
    );
    render(ui);
    await waitFor(() =>
      expect(screen.getAllByText("Opus prod").length).toBeGreaterThan(0),
    );
    // KpiCard renders label as a `text-xs font-medium` sibling above
    // the `text-2xl font-semibold` value — both wrapped in the same
    // `<div class="min-w-0">` panel. Pick each label by its small-text
    // class ("Enabled" / "Disabled" / "Routing Rules" also appear in
    // the table column header below) and read the value off the
    // adjacent value cell.
    const valueOfCard = (label: string): string | null => {
      const labels = screen.getAllByText(label);
      const kpiLabel = labels.find((n) =>
        n.className.includes("text-xs"),
      );
      const valueCell = kpiLabel?.nextElementSibling;
      return valueCell?.textContent ?? null;
    };
    expect(valueOfCard("Enabled")).toBe("1");
    expect(valueOfCard("Disabled")).toBe("1");
    expect(valueOfCard("Routing Rules")).toBe("1");
  });

  it("can create a chat routing rule from the runtime settings UI", async () => {
    vi.mocked(request).mockImplementation((url: string, init?: RequestInit) => {
      if (url === "/models/operations") {
        return Promise.resolve([{ key: "chat", tier: "primary", description: "Chat" }]);
      }
      if (url === "/models/configs") return Promise.resolve([CONFIG_ENABLED]);
      if (url === "/models/routing-rules" && init?.method === "POST") {
        return Promise.resolve({
          id: "rule-chat",
          operation: "chat",
          model_config_id: "cfg-1",
          priority: 0,
          enabled: true,
        });
      }
      if (url === "/models/routing-rules") return Promise.resolve([]);
      return Promise.resolve(undefined);
    });
    const qc = new QueryClient({
      defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
    });
    const ui: ReactElement = (
      <NextIntlClientProvider locale="en" messages={messages}>
        <QueryClientProvider client={qc}>
          <ModelsSettingsPage />
        </QueryClientProvider>
      </NextIntlClientProvider>
    );
    render(ui);
    await waitFor(() =>
      expect(screen.getAllByText("Opus prod").length).toBeGreaterThan(0),
    );

    fireEvent.click(screen.getByRole("button", { name: /^Add Rule$/ }));
    fireEvent.change(screen.getByLabelText("Operation"), {
      target: { value: "chat" },
    });
    fireEvent.click(screen.getByRole("button", { name: /^Create Rule$/ }));

    await waitFor(() =>
      expect(request).toHaveBeenCalledWith(
        "/models/routing-rules",
        expect.objectContaining({
          method: "POST",
          body: JSON.stringify({
            operation: "chat",
            model_config_id: "cfg-1",
            priority: 0,
            enabled: true,
          }),
        }),
      ),
    );
    await waitFor(() =>
      expect(toast.success).toHaveBeenCalledWith("Routing rule created"),
    );
  });

  it("Delete with confirm=true calls DELETE and shows the success toast", async () => {
    setMocks([CONFIG_ENABLED], []);
    confirmMock.mockResolvedValueOnce(true);
    const qc = new QueryClient({
      defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
    });
    const ui: ReactElement = (
      <NextIntlClientProvider locale="en" messages={messages}>
        <QueryClientProvider client={qc}>
          <ModelsSettingsPage />
        </QueryClientProvider>
      </NextIntlClientProvider>
    );
    render(ui);
    await waitFor(() =>
      expect(screen.getAllByText("Opus prod").length).toBeGreaterThan(0),
    );

    // Two Delete buttons can exist (config + rule); since rules=[], only one.
    fireEvent.click(screen.getByRole("button", { name: /^Delete$/ }));
    await waitFor(() => expect(confirmMock).toHaveBeenCalled());
    await waitFor(() =>
      expect(request).toHaveBeenCalledWith(
        "/models/configs/cfg-1",
        expect.objectContaining({ method: "DELETE" }),
      ),
    );
    await waitFor(() =>
      expect(toast.success).toHaveBeenCalledWith("Model config deleted"),
    );
  });

  it("Test config posts the model connection request and surfaces the server message", async () => {
    vi.mocked(request).mockImplementation((url: string) => {
      if (url === "/models/operations") return Promise.resolve([]);
      if (url === "/models/configs")
        return Promise.resolve([CONFIG_ENABLED]);
      if (url === "/models/routing-rules") return Promise.resolve([]);
      if (url === "/models/test") {
        return Promise.resolve({
          ok: true,
          message: "Successfully connected to anthropic / claude-opus-4-7",
        });
      }
      return Promise.resolve(undefined);
    });
    const qc = new QueryClient({
      defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
    });
    const ui: ReactElement = (
      <NextIntlClientProvider locale="en" messages={messages}>
        <QueryClientProvider client={qc}>
          <ModelsSettingsPage />
        </QueryClientProvider>
      </NextIntlClientProvider>
    );
    render(ui);
    await waitFor(() =>
      expect(screen.getAllByText("Opus prod").length).toBeGreaterThan(0),
    );
    fireEvent.click(screen.getByRole("button", { name: /^Test$/ }));
    await waitFor(() =>
      expect(toast.success).toHaveBeenCalledWith(
        "Successfully connected to anthropic / claude-opus-4-7",
      ),
    );
    expect(request).toHaveBeenCalledWith(
      "/models/test",
      expect.objectContaining({
        method: "POST",
        body: JSON.stringify({
          provider: "anthropic",
          model_id: "claude-opus-4-7",
          api_key_env: "ANTHROPIC_API_KEY",
          region: null,
          base_url: null,
        }),
      }),
    );
  });
});
