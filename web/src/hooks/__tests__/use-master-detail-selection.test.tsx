import { describe, expect, it, vi, beforeEach } from "vitest";
import { renderHook, act } from "@testing-library/react";

import {
  DRAFT_ID,
  useMasterDetailSelection,
} from "@/hooks/use-master-detail-selection";

// --- next/navigation mock --------------------------------------------------
//
// jsdom doesn't ship next/navigation. Mock just enough that the hook
// can read a URLSearchParams snapshot and call `router.replace` —
// the same surface real Next.js exposes to client components.

const mockReplace = vi.fn<(url: string) => void>();
let currentParams = new URLSearchParams();

vi.mock("next/navigation", () => ({
  useRouter: () => ({
    replace: (url: string) => {
      mockReplace(url);
      // Reflect the navigation back into the mock so subsequent
      // hook reads see the updated params, mirroring real Next.
      const qs = url.startsWith("?") ? url.slice(1) : url;
      currentParams = new URLSearchParams(qs);
    },
    push: vi.fn(),
    back: vi.fn(),
    forward: vi.fn(),
    refresh: vi.fn(),
    prefetch: vi.fn(),
  }),
  useSearchParams: () => currentParams,
}));

interface Item {
  id: string;
  label: string;
}
const itemId = (i: Item) => i.id;

describe("useMasterDetailSelection", () => {
  beforeEach(() => {
    mockReplace.mockClear();
    currentParams = new URLSearchParams();
  });

  it("auto-selects the first item when URL has no selection", () => {
    const items: Item[] = [{ id: "a", label: "A" }, { id: "b", label: "B" }];
    renderHook(() => useMasterDetailSelection({ items, itemId }));
    // The auto-select effect calls `router.replace("?id=a")`.
    expect(mockReplace).toHaveBeenCalledWith("?id=a");
  });

  it("does not auto-select when items list is empty", () => {
    renderHook(() =>
      useMasterDetailSelection({ items: [] as Item[], itemId }),
    );
    expect(mockReplace).not.toHaveBeenCalled();
  });

  it("returns the matching item for an existing-id URL", () => {
    currentParams = new URLSearchParams("id=b");
    const items: Item[] = [{ id: "a", label: "A" }, { id: "b", label: "B" }];
    const { result } = renderHook(() =>
      useMasterDetailSelection({ items, itemId }),
    );
    expect(result.current.selectedId).toBe("b");
    expect(result.current.selected).toEqual({ id: "b", label: "B" });
    expect(result.current.isDraft).toBe(false);
  });

  it("returns null + isDraft=true for the __new__ sentinel", () => {
    currentParams = new URLSearchParams(`id=${DRAFT_ID}`);
    const items: Item[] = [{ id: "a", label: "A" }];
    const { result } = renderHook(() =>
      useMasterDetailSelection({ items, itemId }),
    );
    expect(result.current.isDraft).toBe(true);
    expect(result.current.selected).toBeNull();
    // Auto-select must skip while the user is in draft state.
    expect(mockReplace).not.toHaveBeenCalled();
  });

  it("returns null when the URL points at an id absent from items", () => {
    currentParams = new URLSearchParams("id=missing");
    const items: Item[] = [{ id: "a", label: "A" }];
    const { result } = renderHook(() =>
      useMasterDetailSelection({ items, itemId }),
    );
    expect(result.current.selectedId).toBe("missing");
    expect(result.current.selected).toBeNull();
  });

  it("setSelection writes the id to the URL", () => {
    currentParams = new URLSearchParams("id=a");
    const items: Item[] = [{ id: "a", label: "A" }, { id: "b", label: "B" }];
    const { result } = renderHook(() =>
      useMasterDetailSelection({ items, itemId }),
    );
    act(() => result.current.setSelection("b"));
    expect(mockReplace).toHaveBeenLastCalledWith("?id=b");
  });

  it("setSelection(null) clears the param and renders `?`", () => {
    currentParams = new URLSearchParams("id=a");
    const items: Item[] = [{ id: "a", label: "A" }];
    const { result } = renderHook(() =>
      useMasterDetailSelection({ items, itemId }),
    );
    act(() => result.current.setSelection(null));
    expect(mockReplace).toHaveBeenLastCalledWith("?");
  });

  it("setSelection(DRAFT_ID) enters the draft state via the URL", () => {
    currentParams = new URLSearchParams("id=a");
    const items: Item[] = [{ id: "a", label: "A" }];
    const { result } = renderHook(() =>
      useMasterDetailSelection({ items, itemId }),
    );
    act(() => result.current.setSelection(DRAFT_ID));
    expect(mockReplace).toHaveBeenLastCalledWith(`?id=${DRAFT_ID}`);
  });

  it("preserves sibling search params when updating selection", () => {
    currentParams = new URLSearchParams("tab=link&id=a");
    const items: Item[] = [{ id: "a", label: "A" }, { id: "b", label: "B" }];
    const { result } = renderHook(() =>
      useMasterDetailSelection({ items, itemId }),
    );
    act(() => result.current.setSelection("b"));
    // Order may vary; check both params survive.
    const arg = mockReplace.mock.lastCall?.[0] ?? "";
    expect(arg).toContain("tab=link");
    expect(arg).toContain("id=b");
  });

  it("respects a custom selectionParam", () => {
    currentParams = new URLSearchParams("entity=a");
    const items: Item[] = [{ id: "a", label: "A" }, { id: "b", label: "B" }];
    const { result } = renderHook(() =>
      useMasterDetailSelection({ items, itemId, selectionParam: "entity" }),
    );
    expect(result.current.selectedId).toBe("a");
    act(() => result.current.setSelection("b"));
    expect(mockReplace).toHaveBeenLastCalledWith("?entity=b");
  });
});
