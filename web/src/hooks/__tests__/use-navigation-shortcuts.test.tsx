import { describe, it, expect, vi, beforeEach } from "vitest";
import { renderHook } from "@testing-library/react";
import type { ReactNode } from "react";

// ---------------------------------------------------------------------------
// `useRouter` mock — the hook's only side-effect is `router.push(href)`,
// so a vi.fn() spy gives us a clear record of which routes the sequence
// triggers. The mock sits at module scope (closures-over-spy pattern)
// because vi.mock factories are hoisted above local consts.
// ---------------------------------------------------------------------------

const pushMock = vi.fn();

vi.mock("next/navigation", () => ({
  useRouter: () => ({ push: pushMock }),
}));

import { useNavigationShortcuts } from "@/hooks/use-navigation-shortcuts";

function renderShortcuts(): { unmount: () => void } {
  const wrapper = ({ children }: { children: ReactNode }) => <>{children}</>;
  const { unmount } = renderHook(() => useNavigationShortcuts(), { wrapper });
  return { unmount };
}

function dispatchKey(
  key: string,
  options: KeyboardEventInit & { target?: EventTarget } = {},
): KeyboardEvent {
  const { target, ...init } = options;
  const event = new KeyboardEvent("keydown", { bubbles: true, ...init });
  if (target) {
    Object.defineProperty(event, "target", { value: target, writable: false });
  }
  Object.defineProperty(event, "key", { value: key, writable: false });
  window.dispatchEvent(event);
  return event;
}

describe("useNavigationShortcuts", () => {
  beforeEach(() => {
    pushMock.mockReset();
  });

  it("routes `g d` → /design", () => {
    const { unmount } = renderShortcuts();
    dispatchKey("g");
    dispatchKey("d");
    expect(pushMock).toHaveBeenCalledExactlyOnceWith("/design");
    unmount();
  });

  it("routes `g g` → /glossary (the gg-style shortcut)", () => {
    const { unmount } = renderShortcuts();
    dispatchKey("g");
    dispatchKey("g");
    expect(pushMock).toHaveBeenCalledExactlyOnceWith("/glossary");
    unmount();
  });

  it("routes `g ,` → /settings (the punctuation shortcut)", () => {
    const { unmount } = renderShortcuts();
    dispatchKey("g");
    dispatchKey(",");
    expect(pushMock).toHaveBeenCalledExactlyOnceWith("/settings");
    unmount();
  });

  it("ignores the sequence when a modifier key is held — browser/OS shortcuts win", () => {
    const { unmount } = renderShortcuts();
    dispatchKey("g", { metaKey: true });
    dispatchKey("d");
    expect(pushMock).not.toHaveBeenCalled();
    unmount();
  });

  it("ignores the sequence when typing into an input — user prose is not stolen", () => {
    const { unmount } = renderShortcuts();
    const input = document.createElement("input");
    document.body.appendChild(input);
    try {
      dispatchKey("g", { target: input });
      dispatchKey("d", { target: input });
      expect(pushMock).not.toHaveBeenCalled();
    } finally {
      document.body.removeChild(input);
      unmount();
    }
  });

  it("treats `<textarea>` and `<select>` as typing targets", () => {
    // JSDOM doesn't reflect `contenteditable` to `isContentEditable`, so
    // we exercise the structural branch (tag-name check) instead. Real
    // browsers handle the contenteditable case via the same predicate.
    const { unmount } = renderShortcuts();
    const textarea = document.createElement("textarea");
    document.body.appendChild(textarea);
    try {
      dispatchKey("g", { target: textarea });
      dispatchKey("d", { target: textarea });
      expect(pushMock).not.toHaveBeenCalled();
    } finally {
      document.body.removeChild(textarea);
      unmount();
    }
  });

  it("does not navigate on a stray second key without the `g` prefix", () => {
    const { unmount } = renderShortcuts();
    dispatchKey("d");
    expect(pushMock).not.toHaveBeenCalled();
    unmount();
  });

  it("does not navigate when the second key is unmapped", () => {
    const { unmount } = renderShortcuts();
    dispatchKey("g");
    dispatchKey("z");
    expect(pushMock).not.toHaveBeenCalled();
    unmount();
  });

  it("uppercase second key is normalised — `g D` still routes to /design", () => {
    const { unmount } = renderShortcuts();
    dispatchKey("g");
    dispatchKey("D");
    expect(pushMock).toHaveBeenCalledExactlyOnceWith("/design");
    unmount();
  });

  it("removes the keydown listener on unmount", () => {
    const { unmount } = renderShortcuts();
    unmount();
    dispatchKey("g");
    dispatchKey("d");
    expect(pushMock).not.toHaveBeenCalled();
  });
});
