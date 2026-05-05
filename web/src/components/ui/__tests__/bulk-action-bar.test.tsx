import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";

import { BulkActionBar } from "@/components/ui/bulk-action-bar";

const baseProps = {
  countLabel: "3 selected",
  clearLabel: "Clear",
  ariaLabel: "Bulk action bar",
  onClear: () => {},
  pending: false,
};

describe("BulkActionBar", () => {
  it("renders aria-hidden when count is 0", () => {
    const { container } = render(
      <BulkActionBar
        {...baseProps}
        count={0}
        actions={[
          { key: "a", label: "Approve", variant: "primary", onClick: vi.fn() },
        ]}
      />,
    );
    // RTL collapses `aria-hidden=true` regions out of the role
    // tree, so query the underlying DOM directly. The bar is
    // mounted (so visibility transitions cleanly) but reads as
    // hidden to assistive tech.
    const region = container.querySelector(
      '[aria-label="Bulk action bar"]',
    );
    expect(region).toHaveAttribute("aria-hidden", "true");
  });

  it("renders aria-hidden=false when count > 0", () => {
    render(
      <BulkActionBar
        {...baseProps}
        count={3}
        actions={[
          { key: "a", label: "Approve", variant: "primary", onClick: vi.fn() },
        ]}
      />,
    );
    const region = screen.getByRole("region", { name: "Bulk action bar" });
    expect(region).toHaveAttribute("aria-hidden", "false");
  });

  it("renders the count label and every action button", () => {
    render(
      <BulkActionBar
        {...baseProps}
        count={3}
        actions={[
          { key: "approve", label: "Approve", variant: "primary", onClick: vi.fn() },
          { key: "reject", label: "Reject", variant: "danger", onClick: vi.fn() },
        ]}
      />,
    );
    expect(screen.getByText("3 selected")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Approve" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Reject" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Clear" })).toBeInTheDocument();
  });

  it("dispatches onClick for the matching action", () => {
    const onApprove = vi.fn();
    const onReject = vi.fn();
    render(
      <BulkActionBar
        {...baseProps}
        count={3}
        actions={[
          { key: "approve", label: "Approve", variant: "primary", onClick: onApprove },
          { key: "reject", label: "Reject", variant: "danger", onClick: onReject },
        ]}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Approve" }));
    expect(onApprove).toHaveBeenCalledTimes(1);
    expect(onReject).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Reject" }));
    expect(onReject).toHaveBeenCalledTimes(1);
  });

  it("dispatches onClear from the clear button", () => {
    const onClear = vi.fn();
    render(
      <BulkActionBar
        {...baseProps}
        count={3}
        onClear={onClear}
        actions={[
          { key: "a", label: "Approve", variant: "primary", onClick: vi.fn() },
        ]}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Clear" }));
    expect(onClear).toHaveBeenCalledTimes(1);
  });

  it("disables every button while pending", () => {
    render(
      <BulkActionBar
        {...baseProps}
        count={3}
        pending
        actions={[
          { key: "a", label: "Approve", variant: "primary", onClick: vi.fn() },
        ]}
      />,
    );
    expect(screen.getByRole("button", { name: "Approve" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Clear" })).toBeDisabled();
  });

  it("falls back to outline variant when an action omits variant", () => {
    render(
      <BulkActionBar
        {...baseProps}
        count={1}
        actions={[{ key: "a", label: "Tag", onClick: vi.fn() }]}
      />,
    );
    // Asserting on an internal class is a bit brittle, but variant is
    // a visual concern and the rendered class name is the contract
    // call sites observe.
    const button = screen.getByRole("button", { name: "Tag" });
    expect(button.className).toMatch(/border|outline/);
  });
});
