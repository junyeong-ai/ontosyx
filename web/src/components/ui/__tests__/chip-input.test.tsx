import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { NextIntlClientProvider } from "next-intl";
import type { ReactElement } from "react";

import messages from "../../../../messages/en.json";
import { ChipInput } from "@/components/ui/chip-input";

function wrap(ui: ReactElement) {
  return render(
    <NextIntlClientProvider locale="en" messages={messages}>
      {ui}
    </NextIntlClientProvider>,
  );
}

describe("ChipInput", () => {
  it("renders one chip per existing value", () => {
    wrap(
      <ChipInput values={["alpha", "beta"]} onChange={vi.fn()} />,
    );
    expect(screen.getByText("alpha")).toBeInTheDocument();
    expect(screen.getByText("beta")).toBeInTheDocument();
  });

  it("Enter commits the draft text as a new chip", () => {
    const onChange = vi.fn();
    wrap(<ChipInput values={["a"]} onChange={onChange} />);
    const input = screen.getByRole("textbox");
    fireEvent.change(input, { target: { value: "b" } });
    fireEvent.keyDown(input, { key: "Enter" });
    expect(onChange).toHaveBeenCalledWith(["a", "b"]);
  });

  it("Comma also commits the draft", () => {
    const onChange = vi.fn();
    wrap(<ChipInput values={[]} onChange={onChange} />);
    const input = screen.getByRole("textbox");
    fireEvent.change(input, { target: { value: "x" } });
    fireEvent.keyDown(input, { key: "," });
    expect(onChange).toHaveBeenCalledWith(["x"]);
  });

  it("blur commits the draft so a typed-then-tabbed value is preserved", () => {
    const onChange = vi.fn();
    wrap(<ChipInput values={[]} onChange={onChange} />);
    const input = screen.getByRole("textbox");
    fireEvent.change(input, { target: { value: "y" } });
    fireEvent.blur(input);
    expect(onChange).toHaveBeenCalledWith(["y"]);
  });

  it("empty draft is not committed (Enter no-op, blur no-op)", () => {
    const onChange = vi.fn();
    wrap(<ChipInput values={["keep"]} onChange={onChange} />);
    const input = screen.getByRole("textbox");
    fireEvent.keyDown(input, { key: "Enter" });
    fireEvent.blur(input);
    expect(onChange).not.toHaveBeenCalled();
  });

  it("duplicate draft is rejected — onChange not called, draft cleared", () => {
    const onChange = vi.fn();
    wrap(<ChipInput values={["alpha"]} onChange={onChange} />);
    const input = screen.getByRole("textbox") as HTMLInputElement;
    fireEvent.change(input, { target: { value: "alpha" } });
    fireEvent.keyDown(input, { key: "Enter" });
    expect(onChange).not.toHaveBeenCalled();
    // Draft slot is reset so the user sees the chip pile, not a stuck duplicate.
    expect(input.value).toBe("");
  });

  it("Backspace at empty input pops the last chip — Mac-native pattern", () => {
    const onChange = vi.fn();
    wrap(<ChipInput values={["a", "b", "c"]} onChange={onChange} />);
    const input = screen.getByRole("textbox");
    fireEvent.keyDown(input, { key: "Backspace" });
    expect(onChange).toHaveBeenCalledWith(["a", "b"]);
  });

  it("Backspace with non-empty draft does NOT pop a chip", () => {
    const onChange = vi.fn();
    wrap(<ChipInput values={["a", "b"]} onChange={onChange} />);
    const input = screen.getByRole("textbox");
    fireEvent.change(input, { target: { value: "draft" } });
    fireEvent.keyDown(input, { key: "Backspace" });
    expect(onChange).not.toHaveBeenCalled();
  });

  it("× button on a chip removes that chip", () => {
    const onChange = vi.fn();
    wrap(<ChipInput values={["a", "b", "c"]} onChange={onChange} />);
    // Each chip has a remove button labelled `Remove <value>`.
    const removeB = screen.getByRole("button", { name: /b/i });
    fireEvent.click(removeB);
    expect(onChange).toHaveBeenCalledWith(["a", "c"]);
  });

  it("paste with newline / comma splits into multiple chips", () => {
    const onChange = vi.fn();
    wrap(<ChipInput values={[]} onChange={onChange} />);
    const input = screen.getByRole("textbox");
    fireEvent.paste(input, {
      clipboardData: { getData: () => "x\ny,z" },
    });
    // Three commits: x, y, z. `onChange` is called with the cumulative
    // append at each commit step.
    expect(onChange).toHaveBeenCalledTimes(3);
    expect(onChange.mock.calls[0][0]).toEqual(["x"]);
  });

  it("paste of a single token falls through to native input — no preventDefault", () => {
    const onChange = vi.fn();
    wrap(<ChipInput values={[]} onChange={onChange} />);
    const input = screen.getByRole("textbox");
    const event = {
      clipboardData: { getData: () => "single" },
    } as unknown as ClipboardEvent;
    fireEvent.paste(input, event);
    // No commit on plain paste — the user gets to keep typing.
    expect(onChange).not.toHaveBeenCalled();
  });

  it("disabled hides the chip remove buttons + input", () => {
    wrap(<ChipInput values={["a"]} onChange={vi.fn()} disabled />);
    const input = screen.getByRole("textbox") as HTMLInputElement;
    expect(input).toBeDisabled();
  });

  it("custom `format` projects the chip label", () => {
    interface Item {
      kind: string;
      value: string;
    }
    wrap(
      <ChipInput<Item>
        values={[{ kind: "tag", value: "draft" }]}
        onChange={vi.fn()}
        format={(item) => `${item.kind}:${item.value}`}
      />,
    );
    expect(screen.getByText("tag:draft")).toBeInTheDocument();
  });

  it("custom `parse` shapes the committed item", () => {
    const onChange = vi.fn<(next: Array<{ raw: string }>) => void>();
    wrap(
      <ChipInput
        values={[]}
        onChange={onChange}
        parse={(text) => ({ raw: text })}
        format={(item) => item.raw}
      />,
    );
    const input = screen.getByRole("textbox");
    fireEvent.change(input, { target: { value: "x" } });
    fireEvent.keyDown(input, { key: "Enter" });
    expect(onChange).toHaveBeenCalledWith([{ raw: "x" }]);
  });
});
