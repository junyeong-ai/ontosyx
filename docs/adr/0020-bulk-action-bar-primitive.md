# 0020 — `BulkActionBar` as the canonical multi-select cohort surface

**Status:** Accepted

**Date:** 2026-05-05

**Supersedes:** none — codifies the canonical primitive; prior
list pages used inline sticky-bottom `<div>` shapes that drifted
across surfaces.

## Context

Several FE list surfaces (knowledge base, stale-concept proposals,
governance approvals, ambiguity-resolved tab on workbench)
compose the same multi-select cohort interaction:

- A row checkbox column.
- A header tri-state checkbox (checked when every visible row is
  selected, indeterminate when only some).
- A sticky-bottom action bar that slides in when at least one
  row is selected, slides out otherwise.
- Bar buttons that disable while a mutation is pending and
  re-enable on settle.
- Selection state that resets on filter / tab change so a
  leftover selection cannot silently target ids from a
  different cohort.

The first three call-sites each rolled their own `<div
className="fixed inset-x-0 bottom-6 ...">` and the variants
diverged on:

- animation timing (300ms ease-out vs 200ms cubic-bezier),
- pointer-events handling during slide-out (one surface
  blocked clicks for an extra 100ms),
- z-index (one surface sat under the global toast layer),
- `aria-live` semantics (some announced count changes, some
  didn't),
- pluralisation (the count label wasn't always re-rendered
  when the locale switched).

By the third instance the drift was visible to operators
working across surfaces.

## Decision

`BulkActionBar` (`web/src/components/ui/bulk-action-bar.tsx`)
is the only acceptable shape for the multi-select cohort
interaction. The primitive owns animation, pointer-events
gating, z-index, and the `aria-live` pattern; the call-site
supplies the count, the labels, and the action handlers.

```tsx
<BulkActionBar
  count={selectedIds.size}
  countLabel={t("bulkSelectedCount", { count: selectedIds.size })}
  clearLabel={t("bulkClear")}
  ariaLabel={t("bulkBarLabel")}
  actions={[
    { key: "reject", label: t("bulkReject"), variant: "danger", onClick: ... },
    { key: "approve", label: t("bulkApprove"), variant: "primary", onClick: ... },
  ]}
  onClear={clearSelection}
  pending={mutation.isPending}
/>
```

Two design constraints fall out of the contract:

- **Pre-translated strings only.** `countLabel` /
  `clearLabel` / `ariaLabel` / each `actions[].label` arrive
  already localised. The primitive doesn't pull `useTranslations`
  internally because per-locale plural rules are call-site
  domain — `t("bulkSelectedCount", { count })` for a knowledge
  selection might pluralise differently from a proposal
  selection.
- **Selection state is local `Set<string>`.** Each list page
  owns its `useState<Set<string>>(new Set())`; the primitive
  never holds selection internally. Reset-on-filter-change is
  call-site responsibility (the primitive can't know when the
  filter changed); the consequence is that every list page
  pairs the primitive with a `useEffect` that clears selection
  when `filters` change.

## Adoption

Four current call-sites:

1. Knowledge base — bulk reject / approve / archive.
2. Stale-concept proposals — bulk decide.
3. Governance approvals — bulk approve / reject.
4. Workbench `ambiguity-resolved` tab — bulk revoke.

A fifth call-site is added by:

1. Add row checkboxes + header tri-state to the existing list.
2. Wire `useState<Set<string>>(new Set())` selection state.
3. Mount `<BulkActionBar count={selectedIds.size} ... />` at
   the bottom of the list.
4. Pair with `useOptimisticMutation` (per ADR-0019) for the
   bulk action handler.

The inline `<div className="fixed inset-x-0 bottom-6 ...">`
shape is forbidden going forward. `pnpm ui-drift-audit` (CI
gate) catches new violations against a baseline ratchet so
the primitive's contract stays the only path.

## Consequences

- **Behaviour stays consistent.** Operators working across
  surfaces see one bulk-action interaction, not four
  variants.
- **Adding a fifth call-site is mechanical.** No animation
  decisions, no z-index puzzles, no per-surface `aria-live`
  re-derivation.
- **The primitive ratchet keeps the bug surface shrinking.**
  CI rejects new inline shapes; the existing four converged
  on the primitive in one sweep.

## Alternatives considered

- **Headless library (Radix / Ark)** — rejected. The shape is
  small enough that owning the primitive in-tree is cheaper
  than the dependency; the headless library doesn't ship the
  pluralisation / `aria-live` / pending-state contract this
  primitive enforces.
- **Per-surface bar components** — rejected. The drift
  observed in the first three call-sites is the documented
  failure mode.
- **Render the bar inside the toast layer** — rejected. The
  toast layer's lifecycle (auto-dismiss) doesn't match the
  selection lifecycle (operator-driven); the z-index conflict
  appears immediately.

## References

- Linear / Slack bulk-action surfaces (industry pattern)
- Memory entry: `feedback_bulk_action_bar_primitive.md`
- Primitive: `web/src/components/ui/bulk-action-bar.tsx`
- CI gate: `pnpm ui-drift-audit`
