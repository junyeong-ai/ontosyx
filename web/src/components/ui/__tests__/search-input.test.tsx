import { describe, expect, it, vi } from "vitest";
import { render, fireEvent } from "@testing-library/react";
import { Search01Icon } from "@hugeicons/core-free-icons";

import { SearchInput } from "../form-input";

describe("SearchInput", () => {
  it("renders the leading icon hidden from a11y tree", () => {
    const { container } = render(
      <SearchInput leadingIcon={Search01Icon} aria-label="Search items" />,
    );
    const iconWrap = container.querySelector('[aria-hidden="true"]');
    expect(iconWrap).toBeTruthy();
    expect(iconWrap?.querySelector("svg")).toBeTruthy();
  });

  it("forwards typed input via onChange", () => {
    const onChange = vi.fn();
    const { container } = render(
      <SearchInput
        leadingIcon={Search01Icon}
        aria-label="Search items"
        onChange={onChange}
      />,
    );
    const input = container.querySelector("input")!;
    fireEvent.change(input, { target: { value: "abc" } });
    expect(onChange).toHaveBeenCalledTimes(1);
  });

  it("defaults type to `search` (semantic + native clear button on supporting browsers)", () => {
    const { container } = render(
      <SearchInput leadingIcon={Search01Icon} aria-label="Search items" />,
    );
    expect(container.querySelector("input")?.type).toBe("search");
  });

  it("respects a caller-provided type override", () => {
    const { container } = render(
      <SearchInput
        leadingIcon={Search01Icon}
        type="text"
        aria-label="Filter"
      />,
    );
    expect(container.querySelector("input")?.type).toBe("text");
  });

  it("aria-invalid is set when `error` is true", () => {
    const { container } = render(
      <SearchInput
        leadingIcon={Search01Icon}
        aria-label="Search"
        error
      />,
    );
    expect(container.querySelector("input")?.getAttribute("aria-invalid")).toBe(
      "true",
    );
  });

  it("compact density tightens icon offset and input padding", () => {
    const { container } = render(
      <SearchInput
        leadingIcon={Search01Icon}
        density="compact"
        aria-label="Search"
      />,
    );
    const input = container.querySelector("input");
    expect(input?.className).toContain("ps-7");
  });
});
