import { describe, it, expect, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { NextIntlClientProvider } from "next-intl";

import messages from "../../../messages/en.json";
import { ExploreFacetSidebar } from "@/components/workbench/explore/facet-sidebar";

function renderSidebar(
  overrides: Partial<Parameters<typeof ExploreFacetSidebar>[0]> = {},
) {
  const onToggleLabel = vi.fn();
  const onClearLabels = vi.fn();
  const onChangeDepth = vi.fn();
  const onSaveSegment = vi.fn();
  render(
    <NextIntlClientProvider locale="en" messages={messages}>
      <ExploreFacetSidebar
        overview={{
          labels: [
            { label: "Customer", count: 1200 },
            { label: "Order", count: 3400 },
          ],
          relationships: [],
          total_nodes: 4600,
          total_relationships: 0,
          node_properties: [],
          rel_properties: [],
        }}
        loading={false}
        selectedLabels={[]}
        onToggleLabel={onToggleLabel}
        onClearLabels={onClearLabels}
        expandDepth={1}
        onChangeDepth={onChangeDepth}
        onSaveSegment={onSaveSegment}
        {...overrides}
      />
    </NextIntlClientProvider>,
  );
  return { onToggleLabel, onClearLabels, onChangeDepth, onSaveSegment };
}

describe("ExploreFacetSidebar", () => {
  it("renders each type with its count and depth controls", () => {
    renderSidebar();
    expect(screen.getByText("Customer")).toBeDefined();
    expect(screen.getByText("Order")).toBeDefined();
    expect(screen.getByText("1,200")).toBeDefined();
    expect(screen.getByText("3,400")).toBeDefined();
    expect(
      screen.getByRole("radio", { name: /1-hop/i }),
    ).toBeDefined();
    expect(
      screen.getByRole("radio", { name: /3-hop/i }),
    ).toBeDefined();
  });

  it("toggling a type fires onToggleLabel with its name", () => {
    const { onToggleLabel } = renderSidebar();
    fireEvent.click(
      screen.getByRole("button", { name: /^Customer/ }),
    );
    expect(onToggleLabel).toHaveBeenCalledWith("Customer");
  });

  it("save-segment button only renders when selection is non-empty", () => {
    const { onSaveSegment: noShow } = renderSidebar(); // selectedLabels: []
    expect(screen.queryByRole("button", { name: /Save \d type/i })).toBeNull();
    expect(noShow).not.toHaveBeenCalled();

    // Re-render with selection.
    render(
      <NextIntlClientProvider locale="en" messages={messages}>
        <ExploreFacetSidebar
          overview={{
            labels: [{ label: "Customer", count: 10 }],
            relationships: [],
            total_nodes: 10,
            total_relationships: 0,
            node_properties: [],
            rel_properties: [],
          }}
          loading={false}
          selectedLabels={["Customer"]}
          onToggleLabel={vi.fn()}
          onClearLabels={vi.fn()}
          expandDepth={1}
          onChangeDepth={vi.fn()}
          onSaveSegment={vi.fn()}
        />
      </NextIntlClientProvider>,
    );
    expect(
      screen.getByRole("button", { name: /Save 1 type\(s\) as segment/i }),
    ).toBeDefined();
  });

  it("changing depth fires onChangeDepth with the new value", () => {
    const { onChangeDepth } = renderSidebar();
    fireEvent.click(screen.getByRole("radio", { name: /3-hop/i }));
    expect(onChangeDepth).toHaveBeenCalledWith(3);
  });
});
