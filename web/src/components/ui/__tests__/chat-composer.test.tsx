import { describe, expect, it, vi } from "vitest";
import { render, fireEvent } from "@testing-library/react";
import { ChatComposer } from "../form-input";

describe("ChatComposer", () => {
  it("renders the textarea with the supplied value", () => {
    const { container } = render(
      <ChatComposer
        value="hello"
        onChange={() => {}}
        trailing={<button type="button">Send</button>}
      />,
    );
    expect(container.querySelector("textarea")?.value).toBe("hello");
  });

  it("renders the trailing slot in the overlay", () => {
    const { getByText } = render(
      <ChatComposer
        value=""
        onChange={() => {}}
        trailing={<button type="button">Send</button>}
      />,
    );
    expect(getByText("Send")).toBeTruthy();
  });

  it("forwards onChange events", () => {
    const onChange = vi.fn();
    const { container } = render(
      <ChatComposer
        value=""
        onChange={onChange}
        trailing={<button type="button">Send</button>}
      />,
    );
    const ta = container.querySelector("textarea")!;
    fireEvent.change(ta, { target: { value: "x" } });
    expect(onChange).toHaveBeenCalledTimes(1);
  });

  it("forwards onKeyDown so consumers can intercept Enter / Cmd-Enter", () => {
    const onKeyDown = vi.fn();
    const { container } = render(
      <ChatComposer
        value=""
        onChange={() => {}}
        onKeyDown={onKeyDown}
        trailing={<button type="button">Send</button>}
      />,
    );
    fireEvent.keyDown(container.querySelector("textarea")!, { key: "Enter" });
    expect(onKeyDown).toHaveBeenCalledTimes(1);
  });

  it("disabled prop propagates to the underlying textarea", () => {
    const { container } = render(
      <ChatComposer
        value=""
        onChange={() => {}}
        disabled
        trailing={<button type="button">Send</button>}
      />,
    );
    expect(container.querySelector("textarea")?.disabled).toBe(true);
  });

  it("attempts to size the textarea on change without throwing", () => {
    // jsdom doesn't compute layout (`scrollHeight` is always 0), so the
    // resulting height is `0px` — that's fine, the contract is "the
    // resize logic ran without crashing in a non-layout environment".
    const { container } = render(
      <ChatComposer
        value=""
        onChange={() => {}}
        maxRows={2}
        trailing={<button type="button">Send</button>}
      />,
    );
    const ta = container.querySelector("textarea")!;
    expect(() =>
      fireEvent.change(ta, { target: { value: "a\nb\nc\nd" } }),
    ).not.toThrow();
  });
});
