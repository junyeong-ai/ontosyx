import { describe, expect, it, vi } from "vitest";
import { render, fireEvent } from "@testing-library/react";

import { FacetShell } from "../facet-shell";

describe("FacetShell", () => {
  it("renders skeleton lines while loading", () => {
    const { container } = render(
      <FacetShell state={{ kind: "loading" }} />,
    );
    // SkeletonText emits one element per line; default 4 in our wrapper.
    expect(container.querySelectorAll("[aria-hidden]").length).toBeGreaterThan(
      0,
    );
  });

  it("renders the ErrorState surface with retry on error", () => {
    const onRetry = vi.fn();
    const { getByText } = render(
      <FacetShell
        state={{
          kind: "error",
          title: "Could not load samples",
          description: "Network error — try again.",
          onRetry,
          retryLabel: "Retry",
        }}
      />,
    );
    fireEvent.click(getByText("Retry"));
    expect(onRetry).toHaveBeenCalledTimes(1);
  });

  it("renders the empty surface with title + optional description", () => {
    const { getByText, queryByText } = render(
      <FacetShell
        state={{
          kind: "empty",
          title: "No samples yet",
        }}
      />,
    );
    expect(getByText("No samples yet")).toBeTruthy();
    expect(queryByText(/error/i)).toBeNull();
  });

  it("renders ready children verbatim", () => {
    const { getByText } = render(
      <FacetShell
        state={{
          kind: "ready",
          children: <div>actual facet content</div>,
        }}
      />,
    );
    expect(getByText("actual facet content")).toBeTruthy();
  });
});
