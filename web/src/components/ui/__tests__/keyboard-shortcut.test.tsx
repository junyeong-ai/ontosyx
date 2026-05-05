import { beforeEach, describe, expect, it } from "vitest";
import { render } from "@testing-library/react";

import { KeyboardShortcut } from "../keyboard-shortcut";

function pinPlatform(platform: string): void {
  Object.defineProperty(navigator, "platform", {
    value: platform,
    configurable: true,
  });
}

beforeEach(() => {
  pinPlatform("Win32");
});

describe("KeyboardShortcut", () => {
  it("renders the supplied glyph verbatim", () => {
    const { container } = render(<KeyboardShortcut glyph="⌘K" />);
    expect(container.querySelector("kbd")?.textContent).toBe("⌘K");
  });

  it("derives the glyph from `keys` per platform", () => {
    pinPlatform("Win32");
    const { container } = render(<KeyboardShortcut keys="mod+k" />);
    expect(container.querySelector("kbd")?.textContent).toContain("Ctrl+");
    expect(container.querySelector("kbd")?.textContent).toContain("K");
  });

  it("renders ⌘ for `mod` on macOS", () => {
    pinPlatform("MacIntel");
    const { container } = render(<KeyboardShortcut keys="mod+k" />);
    const text = container.querySelector("kbd")?.textContent ?? "";
    expect(text).toContain("⌘");
    expect(text).toContain("K");
  });

  it("uses the kbd HTML element for assistive tech", () => {
    const { container } = render(<KeyboardShortcut glyph="A" />);
    expect(container.firstElementChild?.tagName.toLowerCase()).toBe("kbd");
  });

  it("`outline` variant swaps to bordered chrome", () => {
    const { container } = render(
      <KeyboardShortcut glyph="A" variant="outline" />,
    );
    const kbd = container.querySelector("kbd")!;
    expect(kbd.className).toContain("border-divider");
    expect(kbd.className).not.toContain("bg-surface-inset");
  });

  it("`size=default` widens padding and font", () => {
    const { container } = render(
      <KeyboardShortcut glyph="A" size="default" />,
    );
    expect(container.querySelector("kbd")?.className).toContain("text-xs");
  });

  it("merges caller className without dropping the primitive's classes", () => {
    const { container } = render(
      <KeyboardShortcut glyph="A" className="ms-2" />,
    );
    const kbd = container.querySelector("kbd")!;
    expect(kbd.className).toContain("ms-2");
    expect(kbd.className).toContain("font-mono");
  });
});
