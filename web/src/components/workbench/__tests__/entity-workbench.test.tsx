import { describe, expect, it } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";

import { EntityWorkbench } from "@/components/workbench/entity-workbench";

describe("EntityWorkbench layout", () => {
  it("renders the list and detail panes", () => {
    render(
      <EntityWorkbench
        listPane={<div data-testid="list">list</div>}
        detailPane={<div data-testid="detail">detail</div>}
      />,
    );
    expect(screen.getByTestId("list")).toBeInTheDocument();
    expect(screen.getByTestId("detail")).toBeInTheDocument();
  });

  it("omits the aux pane when none provided — 2-column layout", () => {
    render(
      <EntityWorkbench
        listPane={<div>list</div>}
        detailPane={<div>detail</div>}
      />,
    );
    // No aux pane → no toggle button.
    expect(
      screen.queryByRole("button", { name: /toggle/i }),
    ).not.toBeInTheDocument();
  });

  it("renders the aux pane and a toggle button when supplied", () => {
    render(
      <EntityWorkbench
        listPane={<div>list</div>}
        detailPane={<div>detail</div>}
        auxPane={<div data-testid="aux">aux</div>}
        auxToggleLabel="Toggle aux"
      />,
    );
    expect(screen.getByTestId("aux")).toBeInTheDocument();
    expect(screen.getByLabelText("Toggle aux")).toBeInTheDocument();
  });

  it("aux pane defaults to open and collapses on toggle click", () => {
    render(
      <EntityWorkbench
        listPane={<div>list</div>}
        detailPane={<div>detail</div>}
        auxPane={<div>aux</div>}
        auxToggleLabel="Toggle aux"
      />,
    );
    const auxWrapper = screen.getByText("aux").parentElement!;
    expect(auxWrapper).toHaveAttribute("aria-hidden", "false");
    fireEvent.click(screen.getByLabelText("Toggle aux"));
    expect(auxWrapper).toHaveAttribute("aria-hidden", "true");
  });

  it("auxDefaultOpen=false starts collapsed", () => {
    render(
      <EntityWorkbench
        listPane={<div>list</div>}
        detailPane={<div>detail</div>}
        auxPane={<div>aux</div>}
        auxToggleLabel="Toggle aux"
        auxDefaultOpen={false}
      />,
    );
    const auxWrapper = screen.getByText("aux").parentElement!;
    expect(auxWrapper).toHaveAttribute("aria-hidden", "true");
  });

  it("renders the optional banner above the panes", () => {
    render(
      <EntityWorkbench
        listPane={<div>list</div>}
        detailPane={<div>detail</div>}
        banner={<div data-testid="banner">banner</div>}
      />,
    );
    expect(screen.getByTestId("banner")).toBeInTheDocument();
  });

  it("collapsed aux pane is invisible to AT but the host node still mounts", () => {
    render(
      <EntityWorkbench
        listPane={<div>list</div>}
        detailPane={<div>detail</div>}
        auxPane={<div data-testid="aux">aux</div>}
        auxToggleLabel="Toggle aux"
        auxDefaultOpen={false}
      />,
    );
    // The aux node mounts so its internal `useState` / data fetches
    // don't tear down; the wrapper's aria-hidden + invisible class
    // shields it from AT.
    const auxWrapper = screen.getByTestId("aux").parentElement!;
    expect(auxWrapper.className).toContain("invisible");
    expect(auxWrapper.className).toContain("pointer-events-none");
  });
});
