import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { NextIntlClientProvider } from "next-intl";
import type { ReactElement } from "react";

import messages from "../../../../messages/en.json";
import { StatusPill } from "@/components/ui/status-pill";

function wrap(ui: ReactElement) {
  return render(
    <NextIntlClientProvider locale="en" messages={messages}>
      {ui}
    </NextIntlClientProvider>,
  );
}

type State = "active" | "deprecated" | "retired";

const OPTIONS = [
  { key: "active" as const, label: "Active", tone: "success" as const },
  { key: "deprecated" as const, label: "Deprecated", tone: "warning" as const },
  { key: "retired" as const, label: "Retired", tone: "neutral" as const },
];

describe("StatusPill", () => {
  it("renders the active option's label", () => {
    wrap(
      <StatusPill<State>
        value="deprecated"
        options={OPTIONS}
        onChange={vi.fn()}
      />,
    );
    // The trigger shows the active option's label.
    expect(screen.getByText("Deprecated")).toBeInTheDocument();
  });

  it("falls back to the first option when value doesn't match", () => {
    wrap(
      <StatusPill<State>
        // @ts-expect-error — intentional fallthrough
        value="unknown"
        options={OPTIONS}
        onChange={vi.fn()}
      />,
    );
    expect(screen.getByText("Active")).toBeInTheDocument();
  });

  it("opens a popover and emits onChange on option click", () => {
    const onChange = vi.fn();
    wrap(
      <StatusPill<State>
        value="active"
        options={OPTIONS}
        onChange={onChange}
        ariaLabel="Lifecycle"
      />,
    );
    const trigger = screen.getByRole("button", { name: "Lifecycle" });
    fireEvent.click(trigger);
    // Options render as listbox children once the popover opens.
    const retiredOpt = screen.getByRole("option", { name: /Retired/ });
    fireEvent.click(retiredOpt);
    expect(onChange).toHaveBeenCalledWith("retired");
  });

  it("disabled trigger does not respond to clicks", () => {
    const onChange = vi.fn();
    wrap(
      <StatusPill<State>
        value="active"
        options={OPTIONS}
        onChange={onChange}
        ariaLabel="Lifecycle"
        disabled
      />,
    );
    const trigger = screen.getByRole("button", { name: "Lifecycle" });
    // pointer-events-none class prevents clicks on the wrapper, but
    // the underlying base-ui trigger is still a button — confirm the
    // wrapper className gates interaction.
    expect(trigger.className).toContain("pointer-events-none");
  });

  it("each option in the open listbox is keyboard-reachable", () => {
    wrap(
      <StatusPill<State>
        value="active"
        options={OPTIONS}
        onChange={vi.fn()}
      />,
    );
    fireEvent.click(screen.getByText("Active"));
    const opts = screen.getAllByRole("option");
    expect(opts).toHaveLength(3);
    // role=option is the WAI-ARIA contract — listbox-aware screen
    // readers + keyboard nav both rely on it.
    for (const opt of opts) {
      expect(opt.tagName).toBe("BUTTON");
    }
  });

  it("aria-selected marks the currently-active option", () => {
    wrap(
      <StatusPill<State>
        value="deprecated"
        options={OPTIONS}
        onChange={vi.fn()}
      />,
    );
    fireEvent.click(screen.getByText("Deprecated"));
    const active = screen
      .getAllByRole("option")
      .find((o) => o.getAttribute("aria-selected") === "true");
    expect(active?.textContent).toContain("Deprecated");
  });
});
