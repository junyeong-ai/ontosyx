import { describe, it, expect, vi, beforeEach } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { NextIntlClientProvider } from "next-intl";
import type { ReactElement } from "react";

import messages from "../../../../messages/en.json";

// Stub next/navigation so the component can call `usePathname`
// without a real router tree.
let mockPathname = "/analyze";
vi.mock("next/navigation", () => ({
  usePathname: () => mockPathname,
}));

// Stub the hook — the banner only cares about the flattened
// `{ alerts, visible, dismiss }` triple, so we don't need to wire
// up TanStack Query for the banner's own test.
const hookReturn = {
  alerts: [] as Array<{
    metric: string;
    severity: "warning" | "critical";
    value: number;
    threshold: number;
  }>,
  visible: false,
  dismiss: vi.fn(),
  isLoading: false,
};
vi.mock("@/hooks/use-quality-alerts", () => ({
  useQualityAlerts: () => hookReturn,
}));

import { QualityBanner } from "@/components/quality/quality-banner";

function renderBanner(): void {
  const ui: ReactElement = (
    <NextIntlClientProvider locale="en" messages={messages}>
      <QualityBanner />
    </NextIntlClientProvider>
  );
  render(ui);
}

describe("QualityBanner", () => {
  beforeEach(() => {
    mockPathname = "/analyze";
    hookReturn.alerts = [];
    hookReturn.visible = false;
    hookReturn.dismiss.mockReset();
  });

  it("renders nothing when no alerts are visible", () => {
    const { container } = render(
      <NextIntlClientProvider locale="en" messages={messages}>
        <QualityBanner />
      </NextIntlClientProvider>,
    );
    expect(container).toBeEmptyDOMElement();
  });

  it("renders nothing on the /settings/quality?tab=signals route", () => {
    // Even with active alerts, the banner is hidden on the details
    // page it otherwise links to — avoids a redundant banner right
    // above the full dashboard.
    hookReturn.alerts = [
      {
        metric: "shacl_pass_rate",
        severity: "critical",
        value: 0.5,
        threshold: 0.8,
      },
    ];
    hookReturn.visible = true;
    mockPathname = "/settings/quality?tab=signals";
    const { container } = render(
      <NextIntlClientProvider locale="en" messages={messages}>
        <QualityBanner />
      </NextIntlClientProvider>,
    );
    expect(container).toBeEmptyDOMElement();
  });

  it("renders a critical alert with its metric copy and link", () => {
    hookReturn.alerts = [
      {
        metric: "shacl_pass_rate",
        severity: "critical",
        value: 0.5,
        threshold: 0.8,
      },
    ];
    hookReturn.visible = true;
    renderBanner();
    expect(screen.getByText(/Quality signal: critical/)).toBeInTheDocument();
    // Metric copy interpolates percent-formatted value + threshold.
    expect(
      screen.getByText(/SHACL pass rate dipped to 50% \(below 80%\)/),
    ).toBeInTheDocument();
    const link = screen.getByRole("link", { name: /View signal details/i });
    expect(link).toHaveAttribute("href", "/settings/quality?tab=signals");
  });

  it("reports the count of additional metrics when more than one is alerting", () => {
    hookReturn.alerts = [
      {
        metric: "shacl_pass_rate",
        severity: "critical",
        value: 0.5,
        threshold: 0.8,
      },
      {
        metric: "query_reproducibility",
        severity: "warning",
        value: 0.8,
        threshold: 0.85,
      },
      {
        metric: "concept_hit_rate",
        severity: "warning",
        value: 0.25,
        threshold: 0.3,
      },
    ];
    hookReturn.visible = true;
    renderBanner();
    expect(screen.getByText(/and 2 other metrics/)).toBeInTheDocument();
  });

  it("wires the dismiss button to the hook's dismiss callback", () => {
    hookReturn.alerts = [
      {
        metric: "stale_concept_ratio",
        severity: "warning",
        value: 0.12,
        threshold: 0.1,
      },
    ];
    hookReturn.visible = true;
    renderBanner();
    const dismiss = screen.getByRole("button", { name: /Dismiss/i });
    fireEvent.click(dismiss);
    expect(hookReturn.dismiss).toHaveBeenCalledTimes(1);
  });

  it("renders stale_concept_ratio copy with the 'above' phrasing", () => {
    // The stale-concept metric is reversed — a *higher* value is
    // worse. The metric copy needs the matching direction so
    // operators see "rose above threshold", not "dipped below".
    hookReturn.alerts = [
      {
        metric: "stale_concept_ratio",
        severity: "critical",
        value: 0.25,
        threshold: 0.2,
      },
    ];
    hookReturn.visible = true;
    renderBanner();
    expect(
      screen.getByText(/Stale-concept ratio rose to 25% \(above 20%\)/),
    ).toBeInTheDocument();
  });
});
