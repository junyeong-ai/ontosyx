import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { NextIntlClientProvider } from "next-intl";
import type { ReactElement } from "react";

import messages from "../../../../messages/en.json";
import { CommandPalette } from "@/components/ui/command-palette";
import { commandRegistry } from "@/lib/command-registry";

// next/navigation's `useRouter` isn't available under jsdom — mock
// it so the palette can call `router.push` from a registered
// command without exploding.
vi.mock("next/navigation", () => ({
  useRouter: () => ({
    push: vi.fn(),
    replace: vi.fn(),
    back: vi.fn(),
    forward: vi.fn(),
    refresh: vi.fn(),
    prefetch: vi.fn(),
  }),
}));

function wrap(ui: ReactElement) {
  return render(
    <NextIntlClientProvider locale="en" messages={messages}>
      {ui}
    </NextIntlClientProvider>,
  );
}

describe("CommandPalette", () => {
  beforeEach(() => {
    // Hermetic registry — every test starts with no sources so the
    // suite isn't sensitive to whatever surfaces ran first.
    commandRegistry.clearForTests();
  });

  afterEach(() => {
    commandRegistry.clearForTests();
  });

  it("returns null when closed (no DOM mount)", () => {
    wrap(<CommandPalette open={false} onClose={vi.fn()} />);
    expect(
      screen.queryByRole("dialog", { name: /command palette/i }),
    ).not.toBeInTheDocument();
  });

  it("renders an empty-state when open with no registered sources", () => {
    wrap(<CommandPalette open={true} onClose={vi.fn()} />);
    expect(
      screen.getByText(/no commands match/i, { exact: false }),
    ).toBeInTheDocument();
  });

  it("renders sources grouped by groupLabel with one row per command", () => {
    commandRegistry.register({
      id: "global",
      groupLabel: "General",
      commands: () => [
        { id: "a", label: "Command Alpha", execute: () => {} },
        { id: "b", label: "Command Beta", execute: () => {} },
      ],
    });
    wrap(<CommandPalette open={true} onClose={vi.fn()} />);
    expect(screen.getByText("General")).toBeInTheDocument();
    expect(screen.getByText("Command Alpha")).toBeInTheDocument();
    expect(screen.getByText("Command Beta")).toBeInTheDocument();
  });

  it("typing in the search box filters commands by label", () => {
    commandRegistry.register({
      id: "global",
      groupLabel: "General",
      commands: () => [
        { id: "a", label: "Open project", execute: () => {} },
        { id: "b", label: "Toggle inspector", execute: () => {} },
      ],
    });
    wrap(<CommandPalette open={true} onClose={vi.fn()} />);
    const input = screen.getByRole("textbox");
    fireEvent.change(input, { target: { value: "inspect" } });
    expect(screen.queryByText("Open project")).not.toBeInTheDocument();
    expect(screen.getByText("Toggle inspector")).toBeInTheDocument();
  });

  it("ArrowDown / ArrowUp move the active row within the matched set", () => {
    commandRegistry.register({
      id: "global",
      groupLabel: "General",
      commands: () => [
        { id: "a", label: "First", execute: () => {} },
        { id: "b", label: "Second", execute: () => {} },
      ],
    });
    wrap(<CommandPalette open={true} onClose={vi.fn()} />);
    const input = screen.getByRole("textbox");
    // Initial active row is index 0 → "First". After ArrowDown,
    // index 1 → "Second" reads aria-selected.
    fireEvent.keyDown(input, { key: "ArrowDown" });
    const second = screen.getByText("Second").closest('button[role="option"]');
    expect(second).toHaveAttribute("aria-selected", "true");
    fireEvent.keyDown(input, { key: "ArrowUp" });
    const first = screen.getByText("First").closest('button[role="option"]');
    expect(first).toHaveAttribute("aria-selected", "true");
  });

  it("Enter executes the active command and closes the palette", async () => {
    const onClose = vi.fn();
    const execute = vi.fn();
    commandRegistry.register({
      id: "global",
      groupLabel: "General",
      commands: () => [{ id: "a", label: "Run me", execute }],
    });
    wrap(<CommandPalette open={true} onClose={onClose} />);
    const input = screen.getByRole("textbox");
    fireEvent.keyDown(input, { key: "Enter" });
    expect(onClose).toHaveBeenCalledTimes(1);
    // execute is awaited, so micro-task flush:
    await Promise.resolve();
    expect(execute).toHaveBeenCalledTimes(1);
  });

  it("Escape closes the palette without executing", () => {
    const onClose = vi.fn();
    const execute = vi.fn();
    commandRegistry.register({
      id: "global",
      groupLabel: "General",
      commands: () => [{ id: "a", label: "Run me", execute }],
    });
    wrap(<CommandPalette open={true} onClose={onClose} />);
    const input = screen.getByRole("textbox");
    fireEvent.keyDown(input, { key: "Escape" });
    expect(onClose).toHaveBeenCalledTimes(1);
    expect(execute).not.toHaveBeenCalled();
  });

  it("clicking a row executes that command and closes", async () => {
    const onClose = vi.fn();
    const exec = vi.fn();
    commandRegistry.register({
      id: "global",
      groupLabel: "General",
      commands: () => [{ id: "a", label: "Click me", execute: exec }],
    });
    wrap(<CommandPalette open={true} onClose={onClose} />);
    fireEvent.click(screen.getByText("Click me"));
    expect(onClose).toHaveBeenCalledTimes(1);
    await Promise.resolve();
    expect(exec).toHaveBeenCalledTimes(1);
  });

  it("source order from the registry drives render order", () => {
    commandRegistry.register({
      id: "third",
      groupLabel: "Third",
      order: 30,
      commands: () => [{ id: "c", label: "C", execute: () => {} }],
    });
    commandRegistry.register({
      id: "first",
      groupLabel: "First",
      order: 10,
      commands: () => [{ id: "a", label: "A", execute: () => {} }],
    });
    commandRegistry.register({
      id: "second",
      groupLabel: "Second",
      order: 20,
      commands: () => [{ id: "b", label: "B", execute: () => {} }],
    });
    wrap(<CommandPalette open={true} onClose={vi.fn()} />);
    // Compare document order via compareDocumentPosition — jsdom
    // doesn't compute layout so getBoundingClientRect is unreliable.
    const first = screen.getByText("First");
    const second = screen.getByText("Second");
    const third = screen.getByText("Third");
    expect(
      first.compareDocumentPosition(second) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    expect(
      second.compareDocumentPosition(third) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
  });

  it("backdrop click closes the palette", () => {
    const onClose = vi.fn();
    commandRegistry.register({
      id: "global",
      groupLabel: "General",
      commands: () => [{ id: "a", label: "x", execute: () => {} }],
    });
    wrap(<CommandPalette open={true} onClose={onClose} />);
    // The dialog wrapper itself is the click-outside target.
    const dialog = screen.getByRole("dialog");
    fireEvent.click(dialog);
    expect(onClose).toHaveBeenCalled();
  });

  // Registry-mutation propagation (re-render on new source) is
  // covered structurally by `lib/plugins/__tests__/registry.test.ts`;
  // duplicating it here couples the test to focus-trap's mount-time
  // focus discovery, which has flaky interactions with React 19's
  // batched re-renders under jsdom. Trust the lower-level test.
});
