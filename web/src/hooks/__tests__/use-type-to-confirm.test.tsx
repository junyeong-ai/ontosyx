import { describe, expect, it } from "vitest";
import { act, renderHook } from "@testing-library/react";

import { useTypeToConfirm } from "../use-type-to-confirm";

describe("useTypeToConfirm", () => {
  it("starts empty and unmatched", () => {
    const { result } = renderHook(() => useTypeToConfirm("ontosyx"));
    expect(result.current.value).toBe("");
    expect(result.current.matches).toBe(false);
  });

  it("matches flips true on exact phrase entry", () => {
    const { result } = renderHook(() => useTypeToConfirm("ontosyx"));
    act(() => result.current.onChange("ontos"));
    expect(result.current.matches).toBe(false);
    act(() => result.current.onChange("ontosyx"));
    expect(result.current.matches).toBe(true);
    act(() => result.current.onChange("ontosyx "));
    expect(result.current.matches).toBe(false);
  });

  it("is case-sensitive", () => {
    const { result } = renderHook(() => useTypeToConfirm("ontosyx"));
    act(() => result.current.onChange("Ontosyx"));
    expect(result.current.matches).toBe(false);
  });

  it("reset() drops the typed value back to empty", () => {
    const { result } = renderHook(() => useTypeToConfirm("ontosyx"));
    act(() => result.current.onChange("ontosyx"));
    expect(result.current.matches).toBe(true);
    act(() => result.current.reset());
    expect(result.current.value).toBe("");
    expect(result.current.matches).toBe(false);
  });

  it("changing the phrase resets the typed value automatically", () => {
    const { result, rerender } = renderHook(
      ({ phrase }: { phrase: string }) => useTypeToConfirm(phrase),
      { initialProps: { phrase: "first" } },
    );
    act(() => result.current.onChange("first"));
    expect(result.current.matches).toBe(true);
    rerender({ phrase: "second" });
    // After phrase change the typed value resets to empty so the
    // user has to type the new phrase from scratch — guarding
    // against silent re-arming.
    expect(result.current.value).toBe("");
    expect(result.current.matches).toBe(false);
    act(() => result.current.onChange("second"));
    expect(result.current.matches).toBe(true);
  });
});
