import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, renderHook } from "@testing-library/react";

import { useDraftPersistence } from "../use-draft-persistence";

function clearStorage() {
  if (typeof window !== "undefined") window.localStorage.clear();
}

beforeEach(() => {
  vi.useFakeTimers();
  clearStorage();
});

afterEach(() => {
  vi.useRealTimers();
});

describe("useDraftPersistence", () => {
  it("returns draft=null when nothing is in storage", () => {
    const { result } = renderHook(() =>
      useDraftPersistence<{ x: number }>({ key: "test:none" }),
    );
    expect(result.current.draft).toBeNull();
    expect(result.current.hasDraft).toBe(false);
  });

  it("loads an existing draft on mount", () => {
    window.localStorage.setItem(
      "test:loaded",
      JSON.stringify({ value: { x: 7 }, savedAt: Date.now() }),
    );
    const { result } = renderHook(() =>
      useDraftPersistence<{ x: number }>({ key: "test:loaded" }),
    );
    expect(result.current.draft).toEqual({ x: 7 });
    expect(result.current.hasDraft).toBe(true);
  });

  it("ignores drafts older than the TTL and removes them", () => {
    window.localStorage.setItem(
      "test:stale",
      JSON.stringify({
        value: { x: 1 },
        savedAt: Date.now() - 8 * 24 * 60 * 60 * 1000, // 8 days ago
      }),
    );
    const { result } = renderHook(() =>
      useDraftPersistence<{ x: number }>({ key: "test:stale" }),
    );
    expect(result.current.draft).toBeNull();
    expect(window.localStorage.getItem("test:stale")).toBeNull();
  });

  it("debounces save writes and writes only the latest value", () => {
    const { result } = renderHook(() =>
      useDraftPersistence<string>({
        key: "test:debounce",
        debounceMs: 200,
      }),
    );
    act(() => {
      result.current.save("a");
      result.current.save("ab");
      result.current.save("abc");
    });
    // Pre-debounce: nothing in storage yet.
    expect(window.localStorage.getItem("test:debounce")).toBeNull();
    act(() => {
      vi.advanceTimersByTime(200);
    });
    const stored = JSON.parse(
      window.localStorage.getItem("test:debounce") ?? "null",
    );
    expect(stored.value).toBe("abc");
  });

  it("clear() removes the draft and resets state", () => {
    window.localStorage.setItem(
      "test:clear",
      JSON.stringify({ value: { x: 1 }, savedAt: Date.now() }),
    );
    const { result } = renderHook(() =>
      useDraftPersistence<{ x: number }>({ key: "test:clear" }),
    );
    expect(result.current.hasDraft).toBe(true);
    act(() => {
      result.current.clear();
    });
    expect(result.current.draft).toBeNull();
    expect(result.current.hasDraft).toBe(false);
    expect(window.localStorage.getItem("test:clear")).toBeNull();
  });

  it("flushes pending writes on unmount", () => {
    const { result, unmount } = renderHook(() =>
      useDraftPersistence<string>({
        key: "test:flush",
        debounceMs: 1000,
      }),
    );
    act(() => {
      result.current.save("partial");
    });
    // Mid-debounce — nothing flushed yet.
    expect(window.localStorage.getItem("test:flush")).toBeNull();
    unmount();
    const stored = JSON.parse(
      window.localStorage.getItem("test:flush") ?? "null",
    );
    expect(stored.value).toBe("partial");
  });

  it("re-reads on key change (different draft per resource id)", () => {
    window.localStorage.setItem(
      "test:rule-1",
      JSON.stringify({ value: { name: "first" }, savedAt: Date.now() }),
    );
    window.localStorage.setItem(
      "test:rule-2",
      JSON.stringify({ value: { name: "second" }, savedAt: Date.now() }),
    );
    const { result, rerender } = renderHook(
      ({ key }: { key: string }) =>
        useDraftPersistence<{ name: string }>({ key }),
      { initialProps: { key: "test:rule-1" } },
    );
    expect(result.current.draft).toEqual({ name: "first" });
    rerender({ key: "test:rule-2" });
    expect(result.current.draft).toEqual({ name: "second" });
  });

  it("survives a corrupt JSON entry without throwing", () => {
    window.localStorage.setItem("test:corrupt", "{ not valid json");
    const { result } = renderHook(() =>
      useDraftPersistence<unknown>({ key: "test:corrupt" }),
    );
    expect(result.current.draft).toBeNull();
    expect(result.current.hasDraft).toBe(false);
  });
});
