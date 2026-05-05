import { describe, expect, it, vi } from "vitest";
import { render, fireEvent } from "@testing-library/react";

import { RadioCard, RadioGroup } from "../radio";

describe("RadioCard", () => {
  it("renders title and hint", () => {
    const { getByText } = render(
      <RadioCard
        name="g"
        value="a"
        checked={false}
        onChange={() => {}}
        title="Option A"
        hint="Some helper text"
      />,
    );
    expect(getByText("Option A")).toBeTruthy();
    expect(getByText("Some helper text")).toBeTruthy();
  });

  it("applies brand-tone chrome when checked", () => {
    const { container } = render(
      <RadioCard
        name="g"
        value="a"
        checked
        onChange={() => {}}
        title="A"
      />,
    );
    const label = container.querySelector("label")!;
    expect(label.className).toContain("border-brand-foreground");
    expect(label.className).toContain("bg-brand-surface");
  });

  it("falls back to neutral chrome when unchecked", () => {
    const { container } = render(
      <RadioCard
        name="g"
        value="a"
        checked={false}
        onChange={() => {}}
        title="A"
      />,
    );
    const label = container.querySelector("label")!;
    expect(label.className).toContain("border-divider");
    expect(label.className).toContain("bg-surface-base");
    expect(label.className).not.toContain("bg-brand-surface");
  });

  it("hides the underlying input visually but keeps it for a11y", () => {
    const { container } = render(
      <RadioCard
        name="g"
        value="a"
        checked={false}
        onChange={() => {}}
        title="A"
      />,
    );
    const input = container.querySelector("input")!;
    expect(input.type).toBe("radio");
    expect(input.className).toContain("sr-only");
  });

  it("forwards onChange when the card label is clicked", () => {
    const onChange = vi.fn();
    const { container } = render(
      <RadioCard
        name="g"
        value="a"
        checked={false}
        onChange={onChange}
        title="A"
      />,
    );
    fireEvent.click(container.querySelector("input")!);
    expect(onChange).toHaveBeenCalledTimes(1);
  });
});

describe("RadioGroup", () => {
  it("wraps children in a radiogroup region", () => {
    const { getByRole } = render(
      <RadioGroup ariaLabel="Source kind">
        <RadioCard
          name="src"
          value="pg"
          checked={false}
          onChange={() => {}}
          title="Postgres"
        />
        <RadioCard
          name="src"
          value="csv"
          checked
          onChange={() => {}}
          title="CSV"
        />
      </RadioGroup>,
    );
    const group = getByRole("radiogroup", { name: "Source kind" });
    expect(group.querySelectorAll("input[type=\"radio\"]").length).toBe(2);
  });
});
