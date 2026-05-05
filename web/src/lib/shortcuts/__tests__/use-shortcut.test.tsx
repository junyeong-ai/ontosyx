import { describe, it, expect, vi } from "vitest";
import { render, fireEvent } from "@testing-library/react";

import {
  useShortcut,
  ShortcutDispatcher,
  getRegisteredShortcuts,
  isTypingTarget,
  type ShortcutSpec,
} from "@/lib/shortcuts";

function ShortcutHost({ specs }: { specs: ShortcutSpec[] }) {
  // Each spec slot is a fixed hook call. `useShortcut` accepts
  // `undefined` and short-circuits internally, so rules-of-hooks
  // ordering stays constant across renders even as the test varies
  // how many specs it passes (tests use 1–3 specs).
  useShortcut(specs[0]);
  useShortcut(specs[1]);
  useShortcut(specs[2]);
  return null;
}

function App({ specs }: { specs: ShortcutSpec[] }) {
  return (
    <>
      <ShortcutDispatcher />
      <ShortcutHost specs={specs} />
    </>
  );
}

describe("useShortcut", () => {
  it("registers on mount and unregisters on unmount", () => {
    const handler = vi.fn();
    const spec: ShortcutSpec = {
      id: "test.a",
      keys: ["a"],
      group: "test",
      description: "test.a",
      handler,
    };
    const { unmount } = render(<App specs={[spec]} />);
    expect(getRegisteredShortcuts().some((s) => s.id === "test.a")).toBe(true);
    unmount();
    expect(getRegisteredShortcuts().some((s) => s.id === "test.a")).toBe(false);
  });

  it("dispatcher fires the matching handler on keydown", () => {
    const handler = vi.fn();
    render(
      <App
        specs={[
          {
            id: "test.b",
            keys: ["b"],
            group: "test",
            description: "test.b",
            handler,
          },
        ]}
      />,
    );
    fireEvent.keyDown(window, { key: "b" });
    expect(handler).toHaveBeenCalledTimes(1);
  });

  it("non-matching keys do not call the handler", () => {
    const handler = vi.fn();
    render(
      <App
        specs={[
          {
            id: "test.c",
            keys: ["c"],
            group: "test",
            description: "test.c",
            handler,
          },
        ]}
      />,
    );
    fireEvent.keyDown(window, { key: "x" });
    expect(handler).not.toHaveBeenCalled();
  });

  it("higher priority runs first and the lower-priority handler does not fire", () => {
    const order: string[] = [];
    render(
      <App
        specs={[
          {
            id: "test.lo",
            keys: ["p"],
            group: "test",
            description: "lo",
            handler: () => order.push("lo"),
          },
          {
            id: "test.hi",
            keys: ["p"],
            group: "test",
            description: "hi",
            priority: 100,
            handler: () => order.push("hi"),
          },
        ]}
      />,
    );
    fireEvent.keyDown(window, { key: "p" });
    expect(order).toEqual(["hi"]);
  });

  it("re-rendering the host with a new handler does not duplicate registrations", () => {
    const handler1 = vi.fn();
    const handler2 = vi.fn();
    const baseSpec = {
      id: "test.stable",
      keys: ["s"],
      group: "test",
      description: "stable",
    } as const;
    const { rerender } = render(
      <App specs={[{ ...baseSpec, handler: handler1 }]} />,
    );
    rerender(<App specs={[{ ...baseSpec, handler: handler2 }]} />);
    const matches = getRegisteredShortcuts().filter(
      (s) => s.id === "test.stable",
    );
    expect(matches).toHaveLength(1);
    fireEvent.keyDown(window, { key: "s" });
    // The latest handler must run; the stale one must not.
    expect(handler2).toHaveBeenCalledTimes(1);
    expect(handler1).not.toHaveBeenCalled();
  });

  it("multiple specs with different ids all register", () => {
    const a = vi.fn();
    const b = vi.fn();
    render(
      <App
        specs={[
          {
            id: "test.a1",
            keys: ["1"],
            group: "test",
            description: "a",
            handler: a,
          },
          {
            id: "test.b2",
            keys: ["2"],
            group: "test",
            description: "b",
            handler: b,
          },
        ]}
      />,
    );
    fireEvent.keyDown(window, { key: "1" });
    fireEvent.keyDown(window, { key: "2" });
    expect(a).toHaveBeenCalledTimes(1);
    expect(b).toHaveBeenCalledTimes(1);
  });

  it("respects modifier flags on the event", () => {
    const handler = vi.fn();
    render(
      <App
        specs={[
          {
            id: "test.shift",
            keys: ["shift+a"],
            group: "test",
            description: "shift",
            handler,
          },
        ]}
      />,
    );
    fireEvent.keyDown(window, { key: "a", shiftKey: false });
    expect(handler).not.toHaveBeenCalled();
    fireEvent.keyDown(window, { key: "a", shiftKey: true });
    expect(handler).toHaveBeenCalledTimes(1);
  });

  it("skips when focus is in a typing target unless `fireInTypingTarget`", () => {
    const guarded = vi.fn();
    const allowed = vi.fn();
    render(
      <App
        specs={[
          {
            id: "test.guarded",
            keys: ["g"],
            group: "test",
            description: "guarded",
            handler: guarded,
          },
          {
            id: "test.allowed",
            keys: ["g"],
            group: "test",
            description: "allowed",
            fireInTypingTarget: true,
            // Higher priority wins when both match — but this spec
            // doesn't match because its id differs. We test gating
            // separately by switching focus.
            priority: 50,
            handler: allowed,
          },
        ]}
      />,
    );
    // When dispatched against an INPUT, the default-gated spec is
    // suppressed; the `fireInTypingTarget: true` spec wins by both
    // priority and gate.
    const input = document.createElement("input");
    document.body.appendChild(input);
    fireEvent.keyDown(input, { key: "g" });
    expect(guarded).not.toHaveBeenCalled();
    expect(allowed).toHaveBeenCalledTimes(1);
    document.body.removeChild(input);
  });

  it("`enabled` predicate gates the dispatcher", () => {
    const handler = vi.fn();
    let active = false;
    render(
      <App
        specs={[
          {
            id: "test.gated",
            keys: ["x"],
            group: "test",
            description: "gated",
            enabled: () => active,
            handler,
          },
        ]}
      />,
    );
    fireEvent.keyDown(window, { key: "x" });
    expect(handler).not.toHaveBeenCalled();
    active = true;
    fireEvent.keyDown(window, { key: "x" });
    expect(handler).toHaveBeenCalledTimes(1);
  });

  it("logs a dev-time collision warning for two ids on the same combo", () => {
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    render(
      <App
        specs={[
          {
            id: "test.first",
            keys: ["c"],
            group: "test",
            description: "first",
            handler: vi.fn(),
          },
          {
            id: "test.second",
            keys: ["c"],
            group: "test",
            description: "second",
            handler: vi.fn(),
          },
        ]}
      />,
    );
    expect(warn).toHaveBeenCalled();
    const message = warn.mock.calls[0][0] as string;
    expect(message).toContain("c");
    expect(message).toContain("test.first");
    expect(message).toContain("test.second");
    warn.mockRestore();
  });
});

describe("isTypingTarget", () => {
  it("returns true for INPUT / TEXTAREA / SELECT", () => {
    for (const tag of ["INPUT", "TEXTAREA", "SELECT"]) {
      const el = document.createElement(tag);
      expect(isTypingTarget(el)).toBe(true);
    }
  });

  it("returns true for contenteditable elements", () => {
    const el = document.createElement("div");
    el.contentEditable = "true";
    // jsdom doesn't auto-flip `isContentEditable`; force it.
    Object.defineProperty(el, "isContentEditable", { value: true });
    expect(isTypingTarget(el)).toBe(true);
  });

  it("returns false for plain elements", () => {
    expect(isTypingTarget(document.createElement("button"))).toBe(false);
    expect(isTypingTarget(document.createElement("a"))).toBe(false);
    expect(isTypingTarget(document.body)).toBe(false);
    expect(isTypingTarget(null)).toBe(false);
  });
});
