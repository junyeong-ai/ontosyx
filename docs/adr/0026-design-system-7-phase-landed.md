# 0026 — Design system Φ1–Φ7 landed (token + primitive + page chrome + state + polish + a11y + CI gates)

**Status:** Accepted

**Date:** 2026-05-04

**Supersedes:** the v1 / v2 design-system plans documented in
earlier `docs/design/design-system-plan.md` revisions; the
final blueprint shipped in v3 (clean-slate, no backward
compat) is what this ADR records as committed.

## Context

The first 18 months of FE work shipped patterns ad-hoc across
~150 components. By the audit captured in
`docs/design/design-system-plan.md` v2:

- **Typography**: `text-xs` (12px) appeared 719× as default body
  text vs `text-sm` (14px) at 234×. `text-[10px]` appeared 513×.
- **Radius**: bare `rounded` (4px) used 397× vs `rounded-md`
  231 / `rounded-lg` 172.
- **Empty states**: `<EmptyState>` used 33× but 14+ pages
  rendered plain `<p>{t("empty")}</p>`.
- **Motion**: `motion/react` used only 16×.
- **Accessibility**: `text-emerald-600` foreground on white
  failed WCAG AA at 3.6:1.
- **Heading discipline**: raw `<hN className=...>` JSX
  scattered, with no separation between document outline (`hN`)
  and visual tier (size token).
- **Lexical drift**: 4 violet UI-affordance tabs across
  vocabulary / glossary / quality / ambiguity that had each
  picked a different "highlight" color.

A coordinated greenfield re-shape — no migration shims, no
deprecated exports kept "for safety", every legacy removed in
the same PR that introduced the replacement — was the only
way to converge.

## Decision

Seven phases shipped in sequence, every phase culminating in
a CI gate that ratchets the previous phase's invariants:

### Φ1 — Token foundation

Tailwind v4 `@theme inline` block in `web/src/app/globals.css`
defines the canonical token catalogue:

- **Color**: `--color-foreground-{default,muted,subtle,inverse,
  on-accent,danger}`, `--color-surface-{base,raised,inset,
  overlay}`, `--color-divider`, semantic `--color-{primary,danger,
  warning,success,info}-{solid,surface,border,foreground}`.
  Light / dark scoped via `:root` + `.dark`.
- **Radius**: `--radius-{sm,md,lg,xl,2xl,full}`. Bare `rounded`
  is forbidden (gate enforced).
- **Spacing**: `--spacing-{0..16}` mapped to a 4px grid.
- **Heading sizes**: `--heading-{display,1,2,3,4,5,6}-size` +
  matching `--heading-{...}-line-height`.
- **Motion**: `--ease-out`, `--duration-{fast,normal,slow}`.

### Φ2 — Primitive components

Eight platform-grade primitives shipped as the only acceptable
shape for their concern:

- `<Heading level={N} size={M}>` — separates document outline
  from visual tier; raw `<hN className=...>` blocked at CI.
- `<Button variant={...} loading tooltip>` — single button
  primitive with loading state + a11y tooltip slot.
- `<EmptyState>` + `<PageStateView>` — list / page empty +
  loading + error variants.
- `<FormSection>` — fieldset + legend grouping for dense
  schema-driven editors (the `CollapsibleSection` card-chrome
  variant is reserved for page-level tiles).
- `<SaveBar dirty pending onSave onDiscard>` — sticky-bottom
  save bar replacing per-form submit / cancel buttons.
- `<BulkActionBar>` — multi-select cohort surface (per
  ADR-0020).
- `<Eyebrow>` — small uppercase label above headings.
- `<Tooltip>` — single tooltip primitive replacing scattered
  hover-card / popover variants.

### Φ3 — Page chrome

`WorkbenchPageShell` standardises every workbench page's
padding + max-width container; subtitles inline are
forbidden (replaced by an info-icon Tooltip per
`feedback_workbench_shell_padding`). Page-level `px-N py-N`
chrome is a CI-blocked anti-pattern.

### Φ4 — State system

`useShortcut` registry, `PageStateView`, focus-return on
modal close, `Button.loading` / `Button.tooltip` props.

### Φ5 — Polish

Animation defaults via tokenised easings + durations,
`motion/react` adoption hits 80%+ of long-form list /
modal / panel transitions, contrast emerald foreground swap
to AA-passing pair (`text-emerald-700 dark:text-emerald-400`).

### Φ6 — Accessibility

`focus-trap-react` on every modal, `axe-core` smoke test in
the Playwright suite, every route renders `<main id="main">`
as the skip-link target, `vitest-axe` regression tests on
the primitive surface.

### Φ7 — CI gates

`pnpm gate` runs the cumulative ratchet:

- `typecheck` — `tsc --noEmit`.
- `lint` — biome with `--error-on-warnings`.
- `i18n-audit` + `i18n-parity-audit` + `i18n-dotted-key-audit`.
- `error-code-parity-audit` (per ADR-0017).
- `heading-primitive-audit` — blocks new raw `<hN>`.
- `ui-drift-audit` — ratchets the legacy primitive baseline
  (170 violations frozen; new diff fails CI).
- `contrast-audit` — WCAG AA gate.
- `use-client-audit` — `"use client"` directive on line 1
  (per `feedback_use_client_first_line`).
- `source-size-audit` — file-size envelope.
- `design-rules-audit` — token-vs-arbitrary class.
- `vitest` — 530 tests on the primitive surface.

## Consequences

- **One primitive per concern.** Adding a new heading is one
  `<Heading>` call; adding a new bulk-action surface is one
  `<BulkActionBar>` call (per ADR-0020). Composition replaces
  copy-and-mutate.
- **Token-vs-arbitrary is mechanically enforced.** A new
  `text-[10px]` slips into the codebase only if the
  `design-rules-audit` baseline already includes it (and the
  baseline is committed alongside, so the addition is
  visible to the reviewer).
- **Greenfield shape.** No deprecated exports kept "for
  safety"; the legacy was removed in the same PR that
  introduced the replacement. Reading the FE today, there is
  no parallel-old-system to navigate around.
- **Korean copy is operator-grade.** Accessibility tooling
  (axe-core / vitest-axe) catches contrast + label issues
  on every PR rather than relying on per-PR review.

## Adoption

Currently:

- 8 primitives shipped + adopted across 100+ component
  call-sites.
- 11 CI gates in `pnpm gate`.
- 530 vitest tests on the primitive layer.
- Storybook is *not* shipped; the design-system-plan.md
  documents the component contract directly. A Storybook
  (or Ladle) catalog would land if / when the primitive
  surface grows past the threshold where reading the plan
  + the component file is cheaper than running the catalog
  (~30+ primitives — currently 8).

## Alternatives considered

- **Adopt a third-party design system (Material, Chakra,
  Radix-only)** — rejected. The platform's domain
  (knowledge-graph + ontology editing) doesn't map cleanly
  to retail-app primitives; the customised component layer
  would re-derive most of the platform-specific shape
  anyway.
- **Headless-only library (Ark UI, Base UI alone)** —
  partially adopted (`@base-ui-components/react` for the
  modal / popover / tooltip primitives that benefit from
  battle-tested keyboard handling). The token + primitive
  layer above stays in-tree because it carries the
  domain-specific decisions (workbench shell, BulkActionBar,
  master-detail, …) the headless library doesn't speak.
- **Per-team design system** — rejected. Two teams' worth
  of contributors would diverge by the third sprint; the
  single-system + CI-gate shape is the platform's answer.

## References

- `docs/design/design-system-plan.md` — v3 final blueprint
- ADR-0020 — `BulkActionBar` (one of the primitives)
- ADR-0021 — Master-detail over modal (one of the page-chrome
  contracts)
- ADR-0022 — `PluginRegistry<T>` (the extensibility primitive
  the design system's plugin surfaces build on)
- Memory entries: `feedback_design_principles.md`,
  `feedback_heading_primitive_gate.md`,
  `feedback_workbench_shell_padding.md`,
  `feedback_form_section_chrome_lightweight.md`,
  `feedback_use_client_first_line.md`,
  `feedback_main_landmark_required.md`,
  `feedback_axe_emerald_600_aa_fail.md`
- CI gates: `web/scripts/{heading-primitive,ui-drift,contrast,use-client,source-size,design-rules}-audit.mjs`
- Design system tokens: `web/src/app/globals.css` `@theme inline` block
