/**
 * Phase 5 — axe-core a11y coverage for Phase 4 components.
 *
 * Each new surface shipped in Phase 4 (4.1 Bootstrap, 4.3
 * Ambiguity, 4.4 Explore facet, 4.5 Link-term) gets a smoke
 * accessibility pass here. vitest-axe reports any WCAG
 * violation — the assertions below trip on any such finding.
 *
 * Keeping the fixtures minimal keeps the tests focused on the
 * rendered markup's accessibility (landmarks, labels, contrast
 * semantics) rather than business logic.
 */
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render } from "@testing-library/react";
import { axe } from "vitest-axe";
import { NextIntlClientProvider } from "next-intl";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { ReactElement } from "react";

import messages from "../../../../messages/en.json";
import { LinkTermDropdown } from "@/components/workbench/inspector/link-term-dropdown";
import { ResolutionModal } from "@/components/settings/ambiguity/resolution-modal";
import { ExploreFacetSidebar } from "@/components/workbench/explore/facet-sidebar";
import { StepShell } from "@/app/bootstrap/step-shell";
import { BootstrapProvider } from "@/app/bootstrap/bootstrap-state";
import type { AmbiguityContext } from "@/lib/api/ambiguity";

vi.mock("sonner", () => ({
  toast: { success: vi.fn(), error: vi.fn(), info: vi.fn() },
}));

// `next/navigation` only matters for the Bootstrap Step shell (it
// calls `useRouter`). vi.mock returns a minimal stub covering the
// two hooks StepShell touches.
vi.mock("next/navigation", () => ({
  useRouter: () => ({ push: vi.fn(), back: vi.fn() }),
}));

afterEach(cleanup);

function renderA11y(ui: ReactElement) {
  const qc = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return render(
    <NextIntlClientProvider locale="en" messages={messages}>
      <QueryClientProvider client={qc}>{ui}</QueryClientProvider>
    </NextIntlClientProvider>,
  );
}

describe("A11y — Phase 4 components", () => {
  it("LinkTermDropdown (unbound state) has no axe violations", async () => {
    const { container } = renderA11y(
      <LinkTermDropdown
        ontologyId="ont-1"
        expectedVersion={1}
        ownerKind="node"
        ownerTypeId="Customer"
        propertyId="p-tier"
      />,
    );
    expect(await axe(container)).toHaveNoViolations();
  });

  it("LinkTermDropdown (bound state) has no axe violations", async () => {
    const { container } = renderA11y(
      <LinkTermDropdown
        ontologyId="ont-1"
        expectedVersion={1}
        ownerKind="node"
        ownerTypeId="Customer"
        propertyId="p-tier"
        boundTermId="g-vip"
      />,
    );
    expect(await axe(container)).toHaveNoViolations();
  });

  it("Ambiguity ResolutionModal has no axe violations", async () => {
    const ctx: AmbiguityContext = {
      id: "ctx-1",
      source_id: "src-postgres",
      column: { relation: "orders", column: "status" },
      kind: { kind: "numeric_code" },
      sample_values: ["1", "2"],
      clarification_prompt: "What do these codes mean?",
      detection_source_hash: "sha256:abc",
      detected_at: "2026-04-22T00:00:00Z",
    };
    const { container } = renderA11y(
      <ResolutionModal
        context={ctx}
        active={null}
        onSubmit={() => {}}
        onCancel={() => {}}
      />,
    );
    expect(await axe(container)).toHaveNoViolations();
  });

  it("ExploreFacetSidebar has no axe violations", async () => {
    const { container } = renderA11y(
      <ExploreFacetSidebar
        overview={{
          labels: [{ label: "Customer", count: 10 }],
          relationships: [],
          total_nodes: 10,
          total_relationships: 0,
        }}
        loading={false}
        selectedLabels={["Customer"]}
        onToggleLabel={() => {}}
        onClearLabels={() => {}}
        expandDepth={1}
        onChangeDepth={() => {}}
        onSaveSegment={() => {}}
      />,
    );
    expect(await axe(container)).toHaveNoViolations();
  });

  it("Bootstrap StepShell has no axe violations", async () => {
    const { container } = renderA11y(
      <BootstrapProvider>
        <StepShell
          stepKey="1-pilot"
          nextPath="/bootstrap/2-source"
          canAdvance
          title="Scope the pilot"
          subtitle="Pick a narrow slice to prove the ontology."
        >
          <div>Step body content.</div>
        </StepShell>
      </BootstrapProvider>,
    );
    expect(await axe(container)).toHaveNoViolations();
  });
});
