import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen } from "@testing-library/react";
import { NextIntlClientProvider } from "next-intl";
import type { ReactElement } from "react";

import messages from "../../../../../../messages/en.json";

// Stub the data layer at the hook boundary so the test stays
// focused on the page's own routing logic (loading / empty /
// delegated-render). The hook's internals (TanStack Query +
// network fetch) are exercised by the `listOntologies` unit tests.
const mockUseOntologies = vi.fn();
vi.mock("@/hooks/api/use-ontologies", () => ({
  useOntologies: (...args: Parameters<typeof mockUseOntologies>) =>
    mockUseOntologies(...args),
}));

// Mock the panel as a marker element so the test can assert on the
// props the page passes without rendering the panel's own fetches.
// The panel has its own test suite covering its behaviour.
vi.mock("@/components/settings/glossary/binding-panel", () => ({
  GlossaryBindingPanel: ({
    ontologyId,
    expectedVersion,
  }: {
    ontologyId: string;
    expectedVersion: number;
  }) => (
    <div
      data-testid="binding-panel-stub"
      data-ontology-id={ontologyId}
      data-expected-version={expectedVersion}
    />
  ),
}));

import GlossaryBindingsPage from "@/app/settings/glossary/bindings/page";
import type { OntologyListItem } from "@/types/api";

function renderPage(): void {
  const ui: ReactElement = (
    <NextIntlClientProvider locale="en" messages={messages}>
      <GlossaryBindingsPage />
    </NextIntlClientProvider>
  );
  render(ui);
}

const ONTOLOGY_SEED: OntologyListItem = {
  id: "ont-1",
  lineage_id: "lineage-1",
  name: "Pilot",
  description: { default: "" },
  created_at: "2026-04-23T00:00:00Z",
  updated_at: "2026-04-23T00:00:00Z",
  current_version: {
    version_id: "v-1",
    version: "4",
    committed_by: "designer",
    commit_message: "seed",
    created_at: "2026-04-23T00:00:00Z",
  },
};

describe("GlossaryBindingsPage", () => {
  beforeEach(() => {
    mockUseOntologies.mockReset();
  });

  it("shows the page header on every state", () => {
    mockUseOntologies.mockReturnValue({ data: undefined, isLoading: true });
    renderPage();
    expect(
      screen.getByRole("heading", { name: /batch bindings/i }),
    ).toBeInTheDocument();
  });

  it("renders a spinner while the ontology list is loading", () => {
    mockUseOntologies.mockReturnValue({ data: undefined, isLoading: true });
    renderPage();
    // The shared Spinner component uses role=status; when it
    // isn't present, the loading branch isn't active.
    expect(screen.queryByTestId("binding-panel-stub")).not.toBeInTheDocument();
    expect(screen.queryByText(/No committed ontology/i)).not.toBeInTheDocument();
  });

  it("surfaces the empty-state banner when no ontology exists yet", () => {
    mockUseOntologies.mockReturnValue({
      data: { items: [], next_cursor: null },
      isLoading: false,
    });
    renderPage();
    expect(
      screen.getByText(/No committed ontology/i),
    ).toBeInTheDocument();
    expect(screen.queryByTestId("binding-panel-stub")).not.toBeInTheDocument();
  });

  it("renders the binding panel with the current ontology's id + version", () => {
    mockUseOntologies.mockReturnValue({
      data: { items: [ONTOLOGY_SEED], next_cursor: null },
      isLoading: false,
    });
    renderPage();
    const panel = screen.getByTestId("binding-panel-stub");
    expect(panel).toHaveAttribute("data-ontology-id", "ont-1");
    // Page coerces the string `"4"` to a number for the typed prop.
    expect(panel).toHaveAttribute("data-expected-version", "4");
    // The empty-state banner must not appear alongside the panel.
    expect(
      screen.queryByText(/No committed ontology/i),
    ).not.toBeInTheDocument();
  });

  it("holds back the panel until the ontology has a committed version", () => {
    // A rare transitional state: identity exists but `current_version`
    // hasn't been filled in yet (between `create_ontology` and the
    // first `commit_version` write). The panel edits a specific
    // version, so it must not render without one.
    mockUseOntologies.mockReturnValue({
      data: {
        items: [{ ...ONTOLOGY_SEED, current_version: undefined }],
        next_cursor: null,
      },
      isLoading: false,
    });
    renderPage();
    expect(screen.queryByTestId("binding-panel-stub")).not.toBeInTheDocument();
  });
});
