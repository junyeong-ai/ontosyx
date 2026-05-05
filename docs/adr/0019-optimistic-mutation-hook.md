# 0019 — `useOptimisticMutation` as the canonical FE mutation pattern

**Status:** Accepted

**Date:** 2026-05-05

**Supersedes:** none — this ADR codifies the canonical pattern;
prior FE call-sites used bare `useMutation` with hand-rolled
optimistic flows.

## Context

Every list-page mutation that wants immediate visual feedback
(status flip, row remove, bulk decision) follows the same
sequence:

1. cancel any in-flight refetches for the affected query keys,
2. snapshot the current cache value for rollback,
3. apply an optimistic delta to the cache,
4. on the actual mutation: roll back atomically on error,
5. invalidate the cache after settle so the next read pulls
   the authoritative shape.

Hand-rolled with bare `@tanstack/react-query`'s `useMutation` +
`onMutate` + `onError` + `onSettled`, every callsite picks one
of four flavours of the same flow (no optimism, `setQueryData`
without rollback, `onMutate` without invalidation, or the full
triad). Inconsistency surfaces as bugs that look like:

- A row briefly disappears then reappears on rollback because
  the hook forgot to cancel the in-flight refetch first.
- The optimistic update lands but a subsequent invalidation
  races and overwrites the operator's still-pending edit.
- Server error rolls back, but the toast renders before the
  rollback finishes — the operator sees the old state and
  the error message simultaneously, which reads as "the
  retry succeeded but somehow failed".

## Decision

`useOptimisticMutation<Vars, Data>` (`web/src/hooks/api/use-optimistic-mutation.ts`)
codifies the four-step triad as a single hook every list-page
mutation routes through:

```ts
useOptimisticMutation<Vars, Data>({
  mutationFn,
  queryKeys: [knowledgeKeys.list(filters)],
  optimisticUpdate: (prev, vars) => {
    if (!isExpectedShape(prev)) return prev; // runtime guard
    return /* next cache value */;
  },
});
```

Internal sequence:

1. **`onMutate`** — calls `queryClient.cancelQueries()` for every
   `queryKeys` entry, snapshots the current value via
   `getQueryData`, applies the `optimisticUpdate` to the cache
   via `setQueryData`. Returns the snapshots in `context`.
2. **`onError`** — restores the snapshots from `context` so the
   cache reverts atomically before the toast fires.
3. **`onSettled`** — `invalidateQueries` for every `queryKeys`
   entry so the next subscriber pulls the authoritative shape.

The runtime guard inside `optimisticUpdate` is mandatory:
TanStack Query's cache is loosely typed, so the hook receives
`prev: unknown`. Returning `prev` unchanged when the shape isn't
recognised (the cache was hydrated by a different surface, or
the schema evolved between client deploys) is safer than
unwrapping a missing field at runtime.

## Adoption

Every list page that wants optimistic delete / status / bulk
review pairs with the matching named hook:

- `useDeleteKnowledge`, `useUpdateKnowledgeStatus`,
  `useBulkReviewKnowledge`,
- `useBulkReviewApprovals`,
- `useBulkDecideStaleProposals`,
- (the next BE bulk endpoint lands alongside its FE optimistic
  hook in the same commit).

The bare `useMutation` is forbidden in list-page mutation paths
(approval, knowledge, stale, ambiguity-resolved). Single-row
form submits that don't read from a cached list still use
`useMutation` directly — there's nothing optimistic to do.

## Consequences

- **Behaviour is uniform across surfaces.** Every list-page
  delete / status flip / bulk-decision behaves the same way;
  operators don't have to learn surface-specific quirks.
- **Adding a new bulk endpoint is trivial.** The pattern is
  one hook + one `optimisticUpdate` body; the rest of the
  triad lives in the shared hook.
- **Bug surface shrinks.** The four common failure modes
  (race on cancel, dropped invalidation, missing rollback,
  unwrap-on-evolved-schema) are caught once in the hook
  rather than per call-site.
- **Type-safety stays intact.** `useOptimisticMutation<Vars,
  Data>` is generic; the `Vars` and `Data` types flow through
  the call-site so mismatches surface at compile time.

## Alternatives considered

- **Bare `useMutation` everywhere** — rejected. Inconsistency
  is the documented failure mode; the four flavours observed
  in the wild were all bugs.
- **Server-rendered + page reload after every mutation** —
  rejected. Every list page would feel laggy; the workbench
  surface is fundamentally an SPA.
- **Per-domain bespoke hooks** — rejected. Each domain would
  re-derive the triad and drift independently; the BulkActionBar
  primitive (ADR-0020) demonstrates the same anti-pattern that
  this ADR avoids.

## References

- TanStack Query optimistic updates docs
- Memory entry: `feedback_optimistic_mutation_hook.md`
- Hook: `web/src/hooks/api/use-optimistic-mutation.ts`
