import { describe, it, expect, vi, beforeEach } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { NextIntlClientProvider } from "next-intl";
import type { ReactElement } from "react";

import messages from "../../../messages/en.json";
import { ReanalyzeForm } from "@/components/workbench/bottom-panel/workflow-forms";

vi.mock("sonner", () => ({
  toast: { success: vi.fn(), error: vi.fn() },
}));

interface RenderProps {
  sourceType?: string;
  modeledTablesAvailable?: number;
  modeledOnly?: boolean;
}

function renderForm(props: RenderProps = {}) {
  const setModeledOnly = vi.fn();
  const onSubmit = vi.fn();
  const ui: ReactElement = (
    <NextIntlClientProvider locale="en" messages={messages}>
      <ReanalyzeForm
        sourceType={props.sourceType ?? "postgresql"}
        connectionString="postgresql://localhost:5432/db"
        setConnectionString={vi.fn()}
        schemaName="public"
        setSchemaName={vi.fn()}
        sampleData=""
        setSampleData={vi.fn()}
        repoPath=""
        setRepoPath={vi.fn()}
        repoUrl=""
        setRepoUrl={vi.fn()}
        loading={false}
        onSubmit={onSubmit}
        modeledOnly={props.modeledOnly ?? false}
        setModeledOnly={setModeledOnly}
        modeledTablesAvailable={props.modeledTablesAvailable ?? 0}
      />
    </NextIntlClientProvider>
  );
  return { setModeledOnly, onSubmit, ...render(ui) };
}

describe("ReanalyzeForm modeled-only checkbox", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it("hides the modeled-only checkbox when no tables are modeled", () => {
    renderForm({ modeledTablesAvailable: 0 });
    // Match the checkbox by its accessible name (the localized
    // label, not the literal key). Should NOT be in the DOM.
    expect(
      screen.queryByLabelText(/Re-analyze only the/i),
    ).toBeNull();
  });

  it("renders the modeled-only checkbox when included tables exist", () => {
    renderForm({ modeledTablesAvailable: 3 });
    // Pluralization picks the "other" branch for n != 1.
    const cb = screen.getByLabelText(/Re-analyze only the 3 modeled tables/i);
    expect(cb).toBeDefined();
  });

  it("ticking the checkbox calls setModeledOnly(true)", () => {
    const { setModeledOnly } = renderForm({ modeledTablesAvailable: 2 });
    const cb = screen.getByLabelText(/Re-analyze only the 2 modeled tables/i);
    fireEvent.click(cb);
    expect(setModeledOnly).toHaveBeenCalledWith(true);
  });

  it("submit button label flips when modeledOnly is true", () => {
    renderForm({ modeledTablesAvailable: 2, modeledOnly: true });
    expect(
      screen.getByRole("button", { name: /Reanalyze modeled tables/i }),
    ).toBeDefined();
  });

  it("submit button shows the plain Reanalyze label when modeledOnly is false", () => {
    renderForm({ modeledTablesAvailable: 2, modeledOnly: false });
    // Anchored match: avoid the longer "Reanalyze modeled tables".
    expect(
      screen.getByRole("button", { name: /^Reanalyze$/ }),
    ).toBeDefined();
  });

  it("does not render the modeled-only checkbox for code_repository sources", () => {
    // Reanalyze of a code repo doesn't carry a tabular concept of
    // "modeled tables"; the checkbox should be hidden even when
    // analysis_scope.included is non-empty (e.g. a project that
    // started as a DB source and the operator is poking the form
    // mid-flight).
    renderForm({
      sourceType: "code_repository",
      modeledTablesAvailable: 5,
    });
    expect(
      screen.queryByLabelText(/Re-analyze only the/i),
    ).toBeNull();
  });
});
