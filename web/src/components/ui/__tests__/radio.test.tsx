import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";

import { Radio, RadioGroup } from "@/components/ui/radio";

describe("Radio", () => {
  it("renders a native radio input", () => {
    render(<Radio label="A" name="choice" value="a" checked={false} onChange={vi.fn()} />);
    const r = screen.getByRole("radio");
    expect(r).toHaveAttribute("type", "radio");
  });

  it("checked state flows to the input", () => {
    const { rerender } = render(
      <Radio label="A" name="x" value="a" checked={false} onChange={vi.fn()} />,
    );
    expect(screen.getByRole("radio")).not.toBeChecked();
    rerender(
      <Radio label="A" name="x" value="a" checked onChange={vi.fn()} />,
    );
    expect(screen.getByRole("radio")).toBeChecked();
  });

  it("clicking the label toggles via implicit association", () => {
    const onChange = vi.fn();
    render(
      <Radio
        label="Pick me"
        name="grp"
        value="x"
        checked={false}
        onChange={onChange}
      />,
    );
    fireEvent.click(screen.getByText("Pick me"));
    expect(onChange).toHaveBeenCalled();
  });

  it("layout='vertical' stacks input over label", () => {
    const { container } = render(
      <Radio
        label="V"
        name="grp"
        value="v"
        checked={false}
        onChange={vi.fn()}
        layout="vertical"
      />,
    );
    const labelEl = container.querySelector("label");
    expect(labelEl?.className).toContain("flex-col");
  });

  it("default layout is horizontal", () => {
    const { container } = render(
      <Radio label="H" name="grp" value="h" checked={false} onChange={vi.fn()} />,
    );
    const labelEl = container.querySelector("label");
    expect(labelEl?.className).not.toContain("flex-col");
  });

  it("disabled propagates", () => {
    render(
      <Radio
        label="D"
        name="grp"
        value="d"
        checked={false}
        onChange={vi.fn()}
        disabled
      />,
    );
    expect(screen.getByRole("radio")).toBeDisabled();
  });

  it("forwards ref", () => {
    let ref: HTMLInputElement | null = null;
    render(
      <Radio
        ref={(el) => { ref = el; }}
        label="R"
        name="grp"
        value="r"
        checked={false}
        onChange={vi.fn()}
      />,
    );
    expect(ref).toBeInstanceOf(HTMLInputElement);
  });
});

describe("RadioGroup", () => {
  it("wraps children with role='radiogroup' and aria-label", () => {
    render(
      <RadioGroup ariaLabel="Pick a value">
        <Radio label="A" name="pick" value="a" checked onChange={vi.fn()} />
        <Radio label="B" name="pick" value="b" checked={false} onChange={vi.fn()} />
      </RadioGroup>,
    );
    const group = screen.getByRole("radiogroup", { name: "Pick a value" });
    expect(group).toBeInTheDocument();
    expect(group.querySelectorAll("input[type='radio']").length).toBe(2);
  });

  it("custom className composes after base", () => {
    const { container } = render(
      <RadioGroup ariaLabel="X" className="my-extra">
        <Radio label="A" name="x" value="a" checked onChange={vi.fn()} />
      </RadioGroup>,
    );
    const group = container.querySelector("[role='radiogroup']");
    expect(group?.className).toContain("my-extra");
    expect(group?.className).toContain("flex-wrap");
  });
});
