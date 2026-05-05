import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";

import { Checkbox } from "@/components/ui/checkbox";

describe("Checkbox", () => {
  it("renders a native checkbox input", () => {
    render(<Checkbox label="Agree" checked={false} onChange={vi.fn()} />);
    const cb = screen.getByRole("checkbox");
    expect(cb).toHaveAttribute("type", "checkbox");
  });

  it("`checked` flows to the underlying input", () => {
    const { rerender } = render(
      <Checkbox label="A" checked={false} onChange={vi.fn()} />,
    );
    expect(screen.getByRole("checkbox")).not.toBeChecked();
    rerender(<Checkbox label="A" checked onChange={vi.fn()} />);
    expect(screen.getByRole("checkbox")).toBeChecked();
  });

  it("label wraps the input — clicking the label toggles", () => {
    const onChange = vi.fn();
    render(<Checkbox label="Tick me" checked={false} onChange={onChange} />);
    fireEvent.click(screen.getByText("Tick me"));
    expect(onChange).toHaveBeenCalled();
  });

  it("renders without a label when label prop omitted", () => {
    render(<Checkbox checked={false} onChange={vi.fn()} aria-label="bare" />);
    const cb = screen.getByRole("checkbox", { name: "bare" });
    // No surrounding <label> element when no label prop
    expect(cb.parentElement?.tagName).not.toBe("LABEL");
  });

  it("hint renders below label when provided", () => {
    render(
      <Checkbox
        label="Headline"
        hint="Some explanation"
        checked={false}
        onChange={vi.fn()}
      />,
    );
    expect(screen.getByText("Some explanation")).toBeInTheDocument();
  });

  it("disabled state propagates", () => {
    render(<Checkbox label="X" checked={false} onChange={vi.fn()} disabled />);
    expect(screen.getByRole("checkbox")).toBeDisabled();
  });

  it("align='start' adds mt-0.5 to input for first-line alignment", () => {
    render(
      <Checkbox label="X" checked={false} onChange={vi.fn()} align="start" />,
    );
    expect(screen.getByRole("checkbox").className).toContain("mt-0.5");
  });

  it("forwards ref", () => {
    let ref: HTMLInputElement | null = null;
    render(
      <Checkbox
        ref={(el) => { ref = el; }}
        label="Ref"
        checked={false}
        onChange={vi.fn()}
      />,
    );
    expect(ref).toBeInstanceOf(HTMLInputElement);
  });

  it("focus-visible ring class is present", () => {
    render(<Checkbox label="F" checked={false} onChange={vi.fn()} />);
    const cb = screen.getByRole("checkbox");
    expect(cb.className).toContain("focus-visible:ring-2");
  });
});
