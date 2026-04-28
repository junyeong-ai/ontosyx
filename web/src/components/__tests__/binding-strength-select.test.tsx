import { describe, it, expect, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import { NextIntlClientProvider } from "next-intl";
import type { ReactElement } from "react";

import messages from "../../../messages/en.json";
import { BindingStrengthSelect } from "@/components/binding/binding-strength-select";
import type { PropertyBinding } from "@/types/ontology";

type BindingKind = PropertyBinding["kind"];

function wrap(ui: ReactElement) {
  return render(
    <NextIntlClientProvider locale="en" messages={messages}>
      {ui}
    </NextIntlClientProvider>,
  );
}

describe("BindingStrengthSelect", () => {
  it("enables every strength for value_set and code_system targets", () => {
    for (const kind of ["value_set", "code_system"] as BindingKind[]) {
      const { unmount } = wrap(
        <BindingStrengthSelect
          targetKind={kind}
          value="preferred"
          onChange={vi.fn()}
        />,
      );
      const select = screen.getByRole("combobox") as HTMLSelectElement;
      expect(select.disabled).toBe(false);
      const options = Array.from(select.options).map((o) => o.value);
      expect(options).toEqual(["required", "extensible", "preferred", "example"]);
      expect(select.getAttribute("aria-describedby")).toBeNull();
      unmount();
    }
  });

  it("restricts notation_pattern to required + preferred only", () => {
    wrap(
      <BindingStrengthSelect
        targetKind="notation_pattern"
        value="required"
        onChange={vi.fn()}
      />,
    );
    const select = screen.getByRole("combobox") as HTMLSelectElement;
    expect(select.disabled).toBe(false);
    const options = Array.from(select.options).map((o) => o.value);
    expect(options).toEqual(["required", "preferred"]);
  });

  it.each([
    ["value_range", "Value-range bindings classify values only"],
    ["glossary", "Glossary bindings express semantic links only"],
  ])(
    "disables the control for %s with an aria-describedby reason",
    (kind, expectedSubstring) => {
      wrap(
        <BindingStrengthSelect
          targetKind={kind as BindingKind}
          value="preferred"
          onChange={vi.fn()}
        />,
      );
      const select = screen.getByRole("combobox") as HTMLSelectElement;
      expect(select.disabled).toBe(true);
      const describedBy = select.getAttribute("aria-describedby");
      expect(describedBy).toBeTruthy();
      // The reason node must be present in the DOM and contain the
      // expected explanation; assistive tech reads via aria-describedby.
      const reasonNode = document.getElementById(describedBy!);
      expect(reasonNode).toBeTruthy();
      expect(reasonNode!.textContent).toContain(expectedSubstring);
    },
  );

  it("uses unique element ids when two instances mount in the same tree", () => {
    wrap(
      <div>
        <BindingStrengthSelect
          targetKind="glossary"
          value="preferred"
          onChange={vi.fn()}
        />
        <BindingStrengthSelect
          targetKind="value_range"
          value="preferred"
          onChange={vi.fn()}
        />
      </div>,
    );
    const selects = screen.getAllByRole("combobox") as HTMLSelectElement[];
    expect(selects).toHaveLength(2);
    const ids = selects.map((s) => s.id);
    expect(new Set(ids).size).toBe(2);
    const reasonIds = selects.map((s) => s.getAttribute("aria-describedby"));
    expect(new Set(reasonIds).size).toBe(2);
  });
});
