# 0022 — `PluginRegistry<T>` for FE extensibility surfaces

**Status:** Accepted

**Date:** 2026-05-04

**Supersedes:** none — codifies the canonical pattern; existing
plugin surfaces (command sources, inspector facets) are
retrofitted onto it.

## Context

Several FE surfaces need an extensibility hook so new
contributors can plug into a host without editing the host's
file:

- **Command palette (⌘K)** — new commands ride alongside existing
  ones; the host shouldn't know about every contributor.
- **Inspector facets** — new facet tabs land per-domain
  (canvas, branches, evaluation) without the inspector shell
  re-registering them.
- **Future: i18n catalogue contributions, validator
  contributions, edit-op contributions** — same shape.

The first two surfaces (command palette + inspector facets)
each rolled their own extensibility layer with a
module-singleton, a listener `Set`, and a cached snapshot
array. Three failure modes followed:

- **Drift.** The two implementations diverged on `notify`
  semantics — one fired on every `register` even when the
  contributed item was structurally identical, the other
  deduped by id.
- **Snapshot stability bugs.** `useSyncExternalStore` requires
  the snapshot array to be referentially stable across calls
  that didn't change the data; one of the implementations
  re-allocated on every snapshot read, causing infinite
  re-render loops in surfaces that consumed the snapshot.
- **Unmount cleanup.** Both implementations required
  contributors to remember to call `unregister` in a
  `useEffect` cleanup; missing cleanups produced
  "your contribution is still showing after navigation" bugs.

## Decision

`PluginRegistry<T>` (`web/src/lib/plugins/registry.ts`) is the
generic primitive. Every new extensibility surface builds on
it; module-singleton + listener `Set` + snapshot caching is no
longer reinvented per surface.

```ts
const myRegistry = new PluginRegistry<MyItem>({ compare });
// React: usePlugin(myRegistry, item) handles unmount cleanup.
```

The primitive owns three contracts:

- **Stable snapshot.** `list()` caches the sorted-snapshot
  array and returns the same reference until a register /
  unregister event invalidates it. `useSyncExternalStore`
  reads the cached snapshot directly.
- **Compare-aware notify.** `compare(a, b)` defaults to
  reference equality; a registry for items with rich shape
  (e.g. command sources keyed by id) supplies its own
  comparator so a structural-no-op `register` doesn't fire
  every subscriber.
- **`usePlugin` cleanup.** The companion hook handles the
  `useEffect`-based register / unregister pair so call-sites
  don't have to remember the cleanup arm.

## Adoption

Two registries currently retrofitted onto the primitive:

- **`lib/command-registry.ts`** — Cmd+K palette command
  sources. Surfaces contribute commands by registering a
  `CommandSource` while their subtree is mounted.
- **`components/workbench/inspector/facets/registry.tsx`** —
  inspector facet tabs. Per-mode surfaces contribute their
  facets when mounted.

A new extensibility surface is one new module-level
`PluginRegistry<T>` + a thin `useXxx` companion + the
`usePlugin` mount hook on each contributor.

## Consequences

- **Behaviour stays uniform.** Drift between contributing
  surfaces is structurally impossible — they all read from
  the same primitive.
- **Snapshot stability is mechanical.** `useSyncExternalStore`
  consumers don't have to memo their selectors; the registry
  hands them a stable reference.
- **Unmount cleanup is one helper.** `usePlugin(registry,
  item)` is the only line a contributor needs to write; the
  effect's dependency array gates re-registration on item
  identity changes.
- **Forbidden anti-pattern is named.** Re-implementing the
  module-singleton + listener + snapshot trio is a
  code-review-blocker; the primitive owns it.

## Alternatives considered

- **React Context-based plugin host** — rejected. Context
  re-renders every consumer on registry changes; the
  `useSyncExternalStore` shape only re-renders consumers that
  actually depend on the changed data.
- **Zustand store per registry** — rejected. The store API
  weight is overkill for "a list of registered items"; the
  primitive's three-method surface (`register`, `unregister`,
  `list`) is the smallest tool for the job.
- **Per-surface bespoke registries** — rejected. The drift
  observed in the first two adopters is the documented
  failure mode.

## References

- React `useSyncExternalStore` docs
- Memory entry: `feedback_plugin_registry_pattern.md`
- Primitive: `web/src/lib/plugins/registry.ts`
- Adopter: `web/src/lib/command-registry.ts`
- Adopter: `web/src/components/workbench/inspector/facets/registry.tsx`
