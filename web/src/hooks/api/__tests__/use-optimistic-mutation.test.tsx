import { describe, expect, it, vi } from "vitest";
import { act, renderHook, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { ReactNode } from "react";

import { useOptimisticMutation } from "../use-optimistic-mutation";

function makeWrapper() {
  const qc = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={qc}>{children}</QueryClientProvider>
  );
  return { qc, wrapper };
}

describe("useOptimisticMutation", () => {
  it("applies the optimistic delta before the network call resolves", async () => {
    const { qc, wrapper } = makeWrapper();
    const KEY = ["items"] as const;
    qc.setQueryData<string[]>(KEY, ["a", "b"]);
    const mutationFn = vi
      .fn<(v: { name: string }) => Promise<{ ok: true }>>()
      .mockImplementation(async () => {
        // Inspect cache mid-flight — the optimistic value should already be in.
        expect(qc.getQueryData<string[]>(KEY)).toEqual(["a", "b", "c"]);
        return { ok: true };
      });

    const { result } = renderHook(
      () =>
        useOptimisticMutation<{ name: string }, { ok: true }>({
          mutationFn,
          queryKeys: [KEY],
          optimisticUpdate: <T,>(prev: T | undefined, v: { name: string }) =>
            ([...(prev as unknown as string[] ?? []), v.name] as unknown) as T,
        }),
      { wrapper },
    );

    await act(async () => {
      await result.current.mutateAsync({ name: "c" });
    });
    expect(mutationFn).toHaveBeenCalledTimes(1);
  });

  it("rolls back the snapshot on mutation failure", async () => {
    const { qc, wrapper } = makeWrapper();
    const KEY = ["items"] as const;
    qc.setQueryData<string[]>(KEY, ["a"]);
    const mutationFn = vi.fn(async () => {
      throw new Error("boom");
    });

    const { result } = renderHook(
      () =>
        useOptimisticMutation<{ name: string }, void>({
          mutationFn,
          queryKeys: [KEY],
          optimisticUpdate: <T,>(prev: T | undefined, v: { name: string }) =>
            ([...(prev as unknown as string[] ?? []), v.name] as unknown) as T,
        }),
      { wrapper },
    );

    await act(async () => {
      try {
        await result.current.mutateAsync({ name: "b" });
      } catch {
        /* expected */
      }
    });
    // Snapshot restored: the optimistic delta was reverted.
    expect(qc.getQueryData<string[]>(KEY)).toEqual(["a"]);
  });

  it("invalidates every listed key after settlement", async () => {
    const { qc, wrapper } = makeWrapper();
    const A = ["a"] as const;
    const B = ["b"] as const;
    let aFetches = 0;
    let bFetches = 0;
    qc.setQueryDefaults(A, {
      queryFn: async () => {
        aFetches += 1;
        return "alpha";
      },
    });
    qc.setQueryDefaults(B, {
      queryFn: async () => {
        bFetches += 1;
        return "beta";
      },
    });
    // Prime each key — initial fetch.
    await qc.fetchQuery({ queryKey: A });
    await qc.fetchQuery({ queryKey: B });
    expect(aFetches).toBe(1);
    expect(bFetches).toBe(1);

    const { result } = renderHook(
      () =>
        useOptimisticMutation<void, void>({
          mutationFn: async () => undefined,
          queryKeys: [A, B],
          optimisticUpdate: (prev) => prev,
        }),
      { wrapper },
    );

    await act(async () => {
      await result.current.mutateAsync();
    });
    // `invalidateQueries` triggers a refetch on every observer; in this
    // synthetic test we have no observers, but `getQueryState` will
    // show `isInvalidated`. We assert the invalidation flag instead.
    await waitFor(() => {
      expect(qc.getQueryState(A)?.isInvalidated).toBe(true);
      expect(qc.getQueryState(B)?.isInvalidated).toBe(true);
    });
  });

  it("`skipInvalidate` leaves the cache alone after settlement", async () => {
    const { qc, wrapper } = makeWrapper();
    const KEY = ["x"] as const;
    qc.setQueryData(KEY, "original");

    const { result } = renderHook(
      () =>
        useOptimisticMutation<void, void>({
          mutationFn: async () => undefined,
          queryKeys: [KEY],
          optimisticUpdate: <T,>() => "optimistic" as unknown as T,
          skipInvalidate: true,
        }),
      { wrapper },
    );

    await act(async () => {
      await result.current.mutateAsync();
    });
    // No invalidation, optimistic value persists.
    expect(qc.getQueryData(KEY)).toBe("optimistic");
    expect(qc.getQueryState(KEY)?.isInvalidated).toBe(false);
  });

  it("calls onSuccess after the network call resolves", async () => {
    const { wrapper } = makeWrapper();
    const onSuccess = vi.fn();
    const { result } = renderHook(
      () =>
        useOptimisticMutation<{ x: number }, { ok: true }>({
          mutationFn: async () => ({ ok: true }),
          queryKeys: [["k"]],
          optimisticUpdate: (prev) => prev,
          onSuccess,
        }),
      { wrapper },
    );
    await act(async () => {
      await result.current.mutateAsync({ x: 1 });
    });
    expect(onSuccess).toHaveBeenCalledWith({ ok: true }, { x: 1 });
  });

  it("calls onError after rollback runs", async () => {
    const { qc, wrapper } = makeWrapper();
    const KEY = ["k"] as const;
    qc.setQueryData(KEY, "before");
    const onError = vi.fn();
    const { result } = renderHook(
      () =>
        useOptimisticMutation<void, void>({
          mutationFn: async () => {
            throw new Error("nope");
          },
          queryKeys: [KEY],
          optimisticUpdate: <T,>() => "after" as unknown as T,
          onError,
        }),
      { wrapper },
    );
    await act(async () => {
      try {
        await result.current.mutateAsync();
      } catch {
        /* expected */
      }
    });
    expect(qc.getQueryData(KEY)).toBe("before");
    expect(onError).toHaveBeenCalledTimes(1);
    const [err] = onError.mock.calls[0];
    expect((err as Error).message).toBe("nope");
  });
});
