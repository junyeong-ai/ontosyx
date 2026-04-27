import { describe, it, expect, beforeEach } from "vitest";
import { act, renderHook } from "@testing-library/react";

import {
  BOOTSTRAP_STORAGE_KEY,
  BootstrapProvider,
  __resetBootstrapStore,
  useBootstrap,
} from "@/app/bootstrap/bootstrap-state";

function wrapper({ children }: { children: React.ReactNode }) {
  return <BootstrapProvider>{children}</BootstrapProvider>;
}

describe("BootstrapProvider", () => {
  beforeEach(() => {
    window.localStorage.clear();
    // Reset the module-scope snapshot so each test starts with fresh
    // hydration semantics (the store hydrates lazily on the first
    // subscribe — the reset helper simulates a fresh page load).
    __resetBootstrapStore();
  });

  it("starts in the empty state", () => {
    const { result } = renderHook(() => useBootstrap(), { wrapper });
    expect(result.current.state.pilotName).toBe("");
    expect(result.current.state.completedSteps).toEqual([]);
  });

  it("update merges patches and persists to localStorage", () => {
    const { result } = renderHook(() => useBootstrap(), { wrapper });
    act(() => {
      result.current.update({ pilotName: "Order pilot" });
      result.current.update({ sourceKind: "postgresql" });
    });
    expect(result.current.state.pilotName).toBe("Order pilot");
    expect(result.current.state.sourceKind).toBe("postgresql");
    const persisted = JSON.parse(
      window.localStorage.getItem(BOOTSTRAP_STORAGE_KEY) ?? "null",
    );
    expect(persisted.pilotName).toBe("Order pilot");
  });

  it("markComplete is idempotent + preserves order", () => {
    const { result } = renderHook(() => useBootstrap(), { wrapper });
    act(() => {
      result.current.markComplete("1-pilot");
      result.current.markComplete("2-source");
      result.current.markComplete("1-pilot"); // noop
    });
    expect(result.current.state.completedSteps).toEqual([
      "1-pilot",
      "2-source",
    ]);
  });

  it("reset clears both state and storage", () => {
    const { result } = renderHook(() => useBootstrap(), { wrapper });
    act(() => {
      result.current.update({ pilotName: "foo" });
      result.current.markComplete("1-pilot");
      result.current.reset();
    });
    expect(result.current.state.pilotName).toBe("");
    expect(result.current.state.completedSteps).toEqual([]);
    expect(window.localStorage.getItem(BOOTSTRAP_STORAGE_KEY)).toBeNull();
  });

  it("re-hydrates from localStorage on mount", async () => {
    window.localStorage.setItem(
      BOOTSTRAP_STORAGE_KEY,
      JSON.stringify({ pilotName: "Resumed pilot", completedSteps: ["1-pilot"] }),
    );
    const { result } = renderHook(() => useBootstrap(), { wrapper });
    // Hydration is scheduled on a microtask by the subscribe path so
    // the first render emits the server-matched EMPTY snapshot, then
    // the store notifies and React re-renders with stored values.
    await act(async () => {
      await Promise.resolve();
    });
    expect(result.current.state.pilotName).toBe("Resumed pilot");
    expect(result.current.state.completedSteps).toEqual(["1-pilot"]);
  });

  it("falls back to EMPTY when the stored JSON has a wrong-shape payload", async () => {
    // Simulate a schema drift scenario — the legacy payload had
    // `pilotName` as an array of strings (or a boolean, or anything
    // else the next refactor might mistakenly persist). Zod must
    // reject the whole thing rather than let `pilotName: [...]`
    // flow into React state.
    window.localStorage.setItem(
      BOOTSTRAP_STORAGE_KEY,
      JSON.stringify({ pilotName: ["wrong", "shape"], completedSteps: "not-an-array" }),
    );
    const { result } = renderHook(() => useBootstrap(), { wrapper });
    await act(async () => {
      await Promise.resolve();
    });
    expect(result.current.state.pilotName).toBe("");
    expect(result.current.state.completedSteps).toEqual([]);
  });

  it("falls back to EMPTY when the stored payload is not valid JSON", async () => {
    // Corrupted key — nothing we can do but reset. The wizard
    // continues instead of crashing.
    window.localStorage.setItem(BOOTSTRAP_STORAGE_KEY, "{not-json");
    const { result } = renderHook(() => useBootstrap(), { wrapper });
    await act(async () => {
      await Promise.resolve();
    });
    expect(result.current.state.pilotName).toBe("");
    expect(result.current.state.completedSteps).toEqual([]);
  });

  it("hydrates partial payloads with schema defaults filling the rest", async () => {
    // A legitimate forward-compat scenario: an older version wrote
    // only `pilotName`. Zod's per-field defaults must fill the
    // remaining fields so downstream code never sees `undefined`.
    window.localStorage.setItem(
      BOOTSTRAP_STORAGE_KEY,
      JSON.stringify({ pilotName: "Legacy writer" }),
    );
    const { result } = renderHook(() => useBootstrap(), { wrapper });
    await act(async () => {
      await Promise.resolve();
    });
    expect(result.current.state.pilotName).toBe("Legacy writer");
    expect(result.current.state.sourceKind).toBe("");
    expect(result.current.state.completedSteps).toEqual([]);
  });
});
