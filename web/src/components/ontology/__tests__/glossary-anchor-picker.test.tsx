import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, within } from "@testing-library/react";
import { NextIntlClientProvider } from "next-intl";

import enMessages from "../../../../messages/en.json";
import { GlossaryAnchorPicker } from "@/components/ontology/glossary-anchor-picker";
import type { GlossaryTermDef } from "@/lib/api/edit-ops";

vi.mock("@/hooks/use-locale-chain", () => ({
  useLocaleChain: () => ["en", "ko"],
}));

function term(
  id: string,
  label: string,
  display?: string,
): GlossaryTermDef {
  return {
    id,
    term: { default: label },
    display_name: display ? { default: display } : undefined,
  } as GlossaryTermDef;
}

function renderPicker(
  ui: React.ReactElement,
): ReturnType<typeof render> {
  return render(
    <NextIntlClientProvider locale="en" messages={enMessages}>
      {ui}
    </NextIntlClientProvider>,
  );
}

const FIXTURE: GlossaryTermDef[] = [
  term("gt-customer", "Customer", "Active Customer"),
  term("gt-loyalty", "Loyalty Tier"),
  term("gt-order", "Order"),
];

describe("GlossaryAnchorPicker", () => {
  it("renders selected anchors as chips with their display labels", () => {
    renderPicker(
      <GlossaryAnchorPicker
        value={["gt-customer", "gt-loyalty"]}
        glossary={FIXTURE}
        onChange={() => {}}
      />,
    );
    expect(screen.getByText("Active Customer")).toBeDefined();
    expect(screen.getByText("Loyalty Tier")).toBeDefined();
    // Selected ids appear as monospace badges
    expect(screen.getByText("gt-customer")).toBeDefined();
    expect(screen.getByText("gt-loyalty")).toBeDefined();
  });

  it("flags an anchor whose id is absent from the glossary", () => {
    renderPicker(
      <GlossaryAnchorPicker
        value={["gt-orphan"]}
        glossary={FIXTURE}
        onChange={() => {}}
      />,
    );
    // The orphan chip falls back to the bare id as its label.
    const chips = screen.getAllByText("gt-orphan");
    expect(chips.length).toBeGreaterThan(0);
  });

  it("removes an anchor when its remove button is clicked", () => {
    const onChange = vi.fn();
    renderPicker(
      <GlossaryAnchorPicker
        value={["gt-customer", "gt-loyalty"]}
        glossary={FIXTURE}
        onChange={onChange}
      />,
    );
    fireEvent.click(
      screen.getByRole("button", { name: /Remove anchor \(gt-customer\)/ }),
    );
    expect(onChange).toHaveBeenCalledWith(["gt-loyalty"]);
  });

  it("opens the search popover and filters candidates", () => {
    renderPicker(
      <GlossaryAnchorPicker
        value={[]}
        glossary={FIXTURE}
        onChange={() => {}}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Add anchor" }));
    const search = screen.getByPlaceholderText("Search terms…");
    fireEvent.change(search, { target: { value: "loyal" } });
    // Only the matching term should remain visible.
    expect(screen.getByText("Loyalty Tier")).toBeDefined();
    expect(screen.queryByText("Customer")).toBeNull();
  });

  it("excludes already-selected terms from the candidate list", () => {
    renderPicker(
      <GlossaryAnchorPicker
        value={["gt-customer"]}
        glossary={FIXTURE}
        onChange={() => {}}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Add anchor" }));
    // Already-selected term must not appear among candidates — but it
    // still appears in the chip area, so scope the assertion to the
    // popover listbox.
    const popover = screen.getByPlaceholderText("Search terms…").closest("div")!
      .parentElement!;
    expect(within(popover).queryByText("Active Customer")).toBeNull();
    expect(within(popover).getByText("Loyalty Tier")).toBeDefined();
  });

  it("hides every add/remove affordance when readOnly", () => {
    renderPicker(
      <GlossaryAnchorPicker
        value={["gt-customer"]}
        glossary={FIXTURE}
        onChange={() => {}}
        readOnly
      />,
    );
    expect(screen.queryByRole("button", { name: "Add anchor" })).toBeNull();
    expect(
      screen.queryByRole("button", { name: /Remove anchor/ }),
    ).toBeNull();
  });

  it("emits the new id list when a candidate is picked", () => {
    const onChange = vi.fn();
    renderPicker(
      <GlossaryAnchorPicker
        value={[]}
        glossary={FIXTURE}
        onChange={onChange}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Add anchor" }));
    fireEvent.click(screen.getByText("Order"));
    expect(onChange).toHaveBeenCalledWith(["gt-order"]);
  });
});
