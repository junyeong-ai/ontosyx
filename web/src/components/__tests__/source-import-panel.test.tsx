import { describe, it, expect, vi, beforeEach } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { NextIntlClientProvider } from "next-intl";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { ReactElement } from "react";

import messages from "../../../messages/en.json";
import {
  SourceImportPanel,
  toAnalyzeSelection,
  type SourceImportValue,
} from "@/components/workbench/source-import-panel";
import * as clientModule from "@/lib/api/client";
import type { ProjectSource } from "@/types/projects";

vi.mock("sonner", () => ({
  toast: { success: vi.fn(), error: vi.fn() },
}));

const FIXTURE_SOURCE: ProjectSource = {
  type: "postgresql",
  connection_string: "postgresql://localhost:5432/staged",
  schema: "public",
};

function previewResponse() {
  return {
    data: {
      source_type: "postgresql",
      tables: [
        { name: "audit_log", estimated_row_count: null, column_count: 4, last_modified: null },
        { name: "customers", estimated_row_count: 1000, column_count: 8, last_modified: null },
        { name: "orders", estimated_row_count: 5000, column_count: 12, last_modified: null },
      ],
    },
  };
}

function renderPanel(value: SourceImportValue, onChange = vi.fn()) {
  const qc = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  const ui: ReactElement = (
    <NextIntlClientProvider locale="en" messages={messages}>
      <QueryClientProvider client={qc}>
        <SourceImportPanel
          source={FIXTURE_SOURCE}
          value={value}
          onChange={onChange}
        />
      </QueryClientProvider>
    </NextIntlClientProvider>
  );
  return { onChange, ...render(ui) };
}

describe("SourceImportPanel", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    vi.spyOn(clientModule, "request").mockResolvedValue(previewResponse());
  });

  it("renders three mode tiles — all / subset / staged", () => {
    renderPanel({ mode: "all", selectedTables: [] });
    expect(
      screen.getByRole("radio", { name: /Every table/ }),
    ).toBeDefined();
    expect(
      screen.getByRole("radio", { name: /Selected only/ }),
    ).toBeDefined();
    expect(
      screen.getByRole("radio", { name: /Selected \+ acknowledge rest/ }),
    ).toBeDefined();
  });

  it("staged mode opens the table picker (same as subset)", async () => {
    renderPanel({ mode: "staged", selectedTables: [] });
    await waitFor(() =>
      expect(screen.getByText("customers")).toBeDefined(),
    );
    expect(screen.getByText("audit_log")).toBeDefined();
    expect(screen.getByText("orders")).toBeDefined();
  });

  it("toggling a checkbox in staged mode rounds through onChange", async () => {
    const { onChange } = renderPanel({ mode: "staged", selectedTables: [] });
    await waitFor(() =>
      expect(screen.getByText("customers")).toBeDefined(),
    );

    const customers = screen.getByRole("checkbox", { name: /customers/i });
    fireEvent.click(customers);

    expect(onChange).toHaveBeenCalledWith(
      expect.objectContaining({ selectedTables: ["customers"] }),
    );
  });

  it("flipping mode preserves selected tables", () => {
    const { onChange } = renderPanel({
      mode: "subset",
      selectedTables: ["customers"],
    });
    fireEvent.click(
      screen.getByRole("radio", { name: /Selected \+ acknowledge rest/ }),
    );
    expect(onChange).toHaveBeenCalledWith(
      expect.objectContaining({
        mode: "staged",
        selectedTables: ["customers"],
      }),
    );
  });
});

describe("toAnalyzeSelection", () => {
  it("all mode → { kind: 'all' } regardless of intent", () => {
    expect(
      toAnalyzeSelection({ mode: "all", selectedTables: [] }, "create"),
    ).toEqual({ kind: "all" });
    expect(
      toAnalyzeSelection({ mode: "all", selectedTables: [] }, "extend"),
    ).toEqual({ kind: "all" });
  });

  it("create + subset → { kind: 'subset', tables }", () => {
    expect(
      toAnalyzeSelection(
        { mode: "subset", selectedTables: ["customers", "orders"] },
        "create",
      ),
    ).toEqual({ kind: "subset", tables: ["customers", "orders"] });
  });

  it("create + staged → { kind: 'staged', tables }", () => {
    expect(
      toAnalyzeSelection(
        { mode: "staged", selectedTables: ["customers"] },
        "create",
      ),
    ).toEqual({ kind: "staged", tables: ["customers"] });
  });

  it("extend intent forces { kind: 'extend' } regardless of picker mode", () => {
    expect(
      toAnalyzeSelection(
        { mode: "staged", selectedTables: ["payments"] },
        "extend",
      ),
    ).toEqual({ kind: "extend", tables: ["payments"] });
    expect(
      toAnalyzeSelection(
        { mode: "subset", selectedTables: ["payments"] },
        "extend",
      ),
    ).toEqual({ kind: "extend", tables: ["payments"] });
  });
});
