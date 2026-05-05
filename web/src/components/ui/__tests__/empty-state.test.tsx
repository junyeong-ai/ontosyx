import { describe, expect, it, vi } from "vitest";
import { render, fireEvent } from "@testing-library/react";

import { EmptyState } from "../empty-state";

describe("EmptyState", () => {
  it("renders title and description", () => {
    const { getByText } = render(
      <EmptyState title="No projects" description="Create your first one." />,
    );
    expect(getByText("No projects")).toBeTruthy();
    expect(getByText("Create your first one.")).toBeTruthy();
  });

  it("supplies a default icon when none is provided", () => {
    // Default kind is `no-data` — the icon wrapper should be present.
    const { container } = render(<EmptyState title="Empty" />);
    // SVG appears inside the icon wrap div.
    expect(container.querySelector("svg")).toBeTruthy();
  });

  it("`kind=no-permission` swaps the icon tone to muted surface", () => {
    const { container } = render(
      <EmptyState kind="no-permission" title="Access denied" />,
    );
    const wrap = container.querySelector(".rounded-full");
    expect(wrap?.className).toContain("bg-surface-inset");
  });

  it("`kind=error` swaps to warning tone", () => {
    const { container } = render(
      <EmptyState kind="error" title="Something off" />,
    );
    const wrap = container.querySelector(".rounded-full");
    expect(wrap?.className).toContain("bg-warning-surface");
  });

  it("`kind=first-run` keeps brand tone (affirmative)", () => {
    const { container } = render(
      <EmptyState kind="first-run" title="Welcome!" />,
    );
    const wrap = container.querySelector(".rounded-full");
    expect(wrap?.className).toContain("bg-brand-surface");
  });

  it("calls action.onClick when CTA is clicked", () => {
    const onClick = vi.fn();
    const { getByText } = render(
      <EmptyState
        title="No data"
        action={{ label: "Create", onClick }}
      />,
    );
    fireEvent.click(getByText("Create"));
    expect(onClick).toHaveBeenCalledTimes(1);
  });

  it("renders a secondaryAction alongside the primary", () => {
    const primary = vi.fn();
    const secondary = vi.fn();
    const { getByText } = render(
      <EmptyState
        title="No filter matches"
        kind="no-results"
        action={{ label: "Refine", onClick: primary }}
        secondaryAction={{ label: "Clear filter", onClick: secondary }}
      />,
    );
    fireEvent.click(getByText("Clear filter"));
    expect(secondary).toHaveBeenCalledTimes(1);
    expect(primary).not.toHaveBeenCalled();
  });

  it("variant=compact tightens layout class", () => {
    const { container } = render(
      <EmptyState title="x" variant="compact" />,
    );
    const root = container.firstElementChild as HTMLElement;
    expect(root.className).toContain("gap-2");
    expect(root.className).toContain("p-4");
  });
});
