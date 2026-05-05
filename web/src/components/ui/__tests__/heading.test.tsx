import { describe, expect, it } from "vitest";
import { render } from "@testing-library/react";
import { Heading } from "../heading";

describe("Heading", () => {
  it("renders the requested heading tag for each level", () => {
    for (const level of [1, 2, 3, 4, 5, 6] as const) {
      const { container } = render(<Heading level={level}>title</Heading>);
      const tag = container.firstElementChild;
      expect(tag?.tagName.toLowerCase()).toBe(`h${level}`);
    }
  });

  it("tracks visual size to level by default", () => {
    const { container } = render(<Heading level={3}>title</Heading>);
    expect(container.firstElementChild?.classList.contains("heading-3")).toBe(
      true,
    );
  });

  it("decouples visual size from level when `size` is set", () => {
    // h2 in the outline (correct nesting) but visually heading-5 (compact).
    const { container } = render(
      <Heading level={2} size={5}>
        section title
      </Heading>,
    );
    const tag = container.firstElementChild;
    expect(tag?.tagName.toLowerCase()).toBe("h2");
    expect(tag?.classList.contains("heading-5")).toBe(true);
    expect(tag?.classList.contains("heading-2")).toBe(false);
  });

  it("renders h6 at heading-6 visual tier (smallest section subheader)", () => {
    const { container } = render(<Heading level={6}>label</Heading>);
    expect(container.firstElementChild?.classList.contains("heading-6")).toBe(
      true,
    );
  });

  it("supports the `display` size for hero surfaces", () => {
    const { container } = render(
      <Heading level={1} size="display">
        Welcome
      </Heading>,
    );
    expect(
      container.firstElementChild?.classList.contains("heading-display"),
    ).toBe(true);
  });

  it("merges caller className without dropping the size class", () => {
    const { container } = render(
      <Heading level={1} className="custom-thing">
        title
      </Heading>,
    );
    const list = container.firstElementChild?.classList;
    expect(list?.contains("heading-1")).toBe(true);
    expect(list?.contains("custom-thing")).toBe(true);
  });
});
