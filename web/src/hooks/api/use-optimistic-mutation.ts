"use client";

// `useOptimisticMutation` — TanStack Query wrapper that codifies the
// onMutate / onError / onSettled triad so every mutation uses the
// same rollback discipline.
//
// The bare TanStack Query primitive lets you write four flavours of
// the same flow (no optimistic update; setQueryData without rollback;
// onMutate + onError without invalidation; the full triad). Inspector
// + dashboard + canvas mutations end up shaped slightly differently
// from each other and a future migration that needs to add real
// rollback to all of them would have to walk every call site.
//
// This hook collapses the matrix to one shape:
//   1. `mutationFn` — the network call.
//   2. `queryKeys` — every cache key whose value depends on the
//      mutation. The hook cancels in-flight refetches against them
//      before applying the optimistic delta.
//   3. `optimisticUpdate(prev, variables)` — pure transform that
//      computes the next cache value. Run for every key listed.
//   4. `onError` rolls back exactly the snapshots that were taken;
//      cancellation guarantees the rollback isn't clobbered by a
//      late-arriving server result.
//   5. `onSettled` invalidates the keys so the server's canonical
//      shape replaces the optimistic delta on the next render.
//
// Optional `onSuccess` runs *after* the cache has been invalidated —
// the perfect spot for a success toast or a `router.push` to a
// detail page that's now in cache.

import {
  useMutation,
  useQueryClient,
  type UseMutationOptions,
} from "@tanstack/react-query";

export interface OptimisticMutationOptions<TVariables, TData> {
  /** The network call. Receives the caller's input; returns the canonical server value. */
  mutationFn: (variables: TVariables) => Promise<TData>;
  /**
   * Every cache key whose value is touched by this mutation. The
   * hook cancels in-flight queries on each, takes a snapshot, and
   * rolls every snapshot back atomically on error.
   */
  queryKeys: ReadonlyArray<readonly unknown[]>;
  /**
   * Pure transform from prior cache value to next. Runs once per
   * key in `queryKeys`. The same transform fires for every key —
   * if your mutation needs a per-key transform, list multiple
   * `useOptimisticMutation` calls instead, one per shape.
   */
  optimisticUpdate: <T>(prev: T | undefined, variables: TVariables) => T | undefined;
  /** Callback after the cache has been invalidated and the server result is in flight. */
  onSuccess?: (data: TData, variables: TVariables) => void;
  /** Callback after the optimistic delta has been rolled back. */
  onError?: (error: Error, variables: TVariables) => void;
  /** Skip the post-settle `invalidateQueries`. Default false — almost everyone wants it. */
  skipInvalidate?: boolean;
  /** Forwarded to `useMutation` for retry / mutationKey / etc. */
  mutationOptions?: Omit<
    UseMutationOptions<TData, Error, TVariables, OptimisticContext>,
    "mutationFn" | "onMutate" | "onError" | "onSettled" | "onSuccess"
  >;
}

interface OptimisticContext {
  /** Per-key snapshot taken before the optimistic delta was applied. */
  snapshots: Array<{ key: readonly unknown[]; value: unknown }>;
}

export function useOptimisticMutation<TVariables, TData>(
  opts: OptimisticMutationOptions<TVariables, TData>,
) {
  const qc = useQueryClient();
  return useMutation<TData, Error, TVariables, OptimisticContext>({
    mutationFn: opts.mutationFn,
    onMutate: async (variables) => {
      // Cancel every in-flight refetch first so a late server result
      // can't overwrite our optimistic delta and confuse the user.
      await Promise.all(
        opts.queryKeys.map((key) => qc.cancelQueries({ queryKey: key })),
      );
      const snapshots = opts.queryKeys.map((key) => ({
        key,
        value: qc.getQueryData(key),
      }));
      for (const key of opts.queryKeys) {
        qc.setQueryData(key, (prev: unknown) =>
          opts.optimisticUpdate(prev, variables),
        );
      }
      return { snapshots };
    },
    onError: (error, variables, context) => {
      if (context) {
        for (const { key, value } of context.snapshots) {
          qc.setQueryData(key, value);
        }
      }
      opts.onError?.(error, variables);
    },
    onSettled: () => {
      if (opts.skipInvalidate) return;
      for (const key of opts.queryKeys) {
        qc.invalidateQueries({ queryKey: key });
      }
    },
    onSuccess: (data, variables) => {
      opts.onSuccess?.(data, variables);
    },
    ...opts.mutationOptions,
  });
}
