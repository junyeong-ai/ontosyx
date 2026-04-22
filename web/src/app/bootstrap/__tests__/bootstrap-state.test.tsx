import { describe, it, expect, beforeEach } from "vitest";
import { act, renderHook } from "@testing-library/react";

import {
  BootstrapProvider,
  useBootstrap,
} from "@/app/bootstrap/bootstrap-state";

function wrapper({ children }: { children: React.ReactNode }) {
  return <BootstrapProvider>{children}</BootstrapProvider>;
}

describe("BootstrapProvider", () => {
  beforeEach(() => {
    window.localStorage.clear();
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
      window.localStorage.getItem("ontosyx.bootstrap.v1") ?? "null",
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
    expect(window.localStorage.getItem("ontosyx.bootstrap.v1")).toBeNull();
  });

  it("re-hydrates from localStorage on mount", () => {
    window.localStorage.setItem(
      "ontosyx.bootstrap.v1",
      JSON.stringify({ pilotName: "Resumed pilot", completedSteps: ["1-pilot"] }),
    );
    const { result } = renderHook(() => useBootstrap(), { wrapper });
    // Hydration runs in an effect, triggering a re-render. The hook
    // result updates, so assert via the latest `result.current`.
    expect(result.current.state.pilotName).toBe("Resumed pilot");
    expect(result.current.state.completedSteps).toEqual(["1-pilot"]);
  });
});
