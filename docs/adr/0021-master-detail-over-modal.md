# 0021 — Master-detail over modal for vocabulary / list CRUD surfaces

**Status:** Accepted

**Date:** 2026-05-04

**Supersedes:** the prior modal-based create / edit pattern on
the vocabulary surfaces (CodeSystem, ValueSet, ConceptMap,
NotationPattern, Rule, mappings, glossary). Modals on those
surfaces are removed.

## Context

The first cut of every vocabulary CRUD surface (CodeSystem,
ValueSet, ConceptMap, NotationPattern, Rule, mappings,
glossary) used a list-then-modal pattern: a list page with
a "Create" button that opened a modal, plus a row "Edit"
action that re-opened the same modal. Three failure modes
followed:

- **No comparison context.** Editing a value set required
  closing the modal to see the surrounding entries; "is this
  field different from the sibling?" was a multi-click
  question.
- **No usage visibility.** "Where is this code system
  referenced?" couldn't be answered while the operator was
  editing — the modal occluded the list and the rest of the
  page.
- **Deep-link unfriendly.** A modal can't be the canonical URL
  for "the active code system". Sharing a link to "this code
  system in this state" required a screenshot.

Industry practice for vocabulary / list CRUD has converged on
the master-detail pattern: list pane on the left, an
always-visible editor pane on the right, optionally a
usage / preview pane next to the editor. Linear's settings,
Stripe Dashboard, Notion's database editing, Sanity Studio
content panels, Figma's style library all share the shape.
Modals on those surfaces are reserved for destructive
confirmations only.

## Decision

Vocabulary / list CRUD surfaces use **master-detail split**:

- **List pane** on the left renders every entity, search /
  filter at the top, row-click selects the entity into the
  editor.
- **Editor pane** in the centre is always visible. Selecting a
  row populates it; the URL writes back as `?id=<entityId>` so
  back-button navigation behaves naturally. `?id=__new__` is
  the draft-create state — clicking "New" doesn't open a
  modal, it puts the editor in the new-entity state.
- **Optional usage pane** on the right surfaces "where is this
  entity referenced from", lazy-loaded so the surface stays
  fast on entities the operator opens-and-closes.

`MasterDetailEntityPage`
(`web/src/components/vocabulary/master-detail-entity-page.tsx`)
is the canonical scaffold; `EntityWorkbench`
(`web/src/components/workbench/entity-workbench.tsx`) is the
lower-level shell that handles only pane layout. New CRUD
surfaces drop into one of those — modals as the create / edit
flow are forbidden.

Modals on these surfaces are reserved for **destructive
confirmations only** (`Delete this code system?` with a
single Cancel / Confirm choice). Anything that captures form
state lives in the editor pane.

## Save semantics

Editor panes close with `<SaveBar dirty={...} pending={...}
onSave onDiscard />`. The bar slides in from the bottom only
when there are unsaved changes — the same pattern as Linear,
Sanity, Notion. Explicit submit / cancel buttons at the bottom
of an editor are forbidden; the route through `SaveBar` keeps
the dirty / save state consistent across surfaces.

Dirty calculation is `snapshotEqual(currentSnapshot,
initialSnapshot)` (`@/lib/snapshot-equal`) where each
snapshot is a `useMemo`'d object of every editable slot.
`JSON.stringify` is forbidden as a dirty check —
implementation-defined key order and `undefined`-vs-absent
collapse both produce false dirty / false clean. The same
snapshot drives `useDraftPersistence` auto-save.

## Consequences

- **Comparison + usage context is one URL.** The operator sees
  the list, the editor, and the usage pane simultaneously;
  navigation between entities is one click without losing
  surrounding context.
- **Deep-link works naturally.** `?id=<entity>` is the URL
  contract; sharing "this entity in this state" is a copy-
  link action.
- **New CRUD surfaces are mechanical.** The scaffold owns
  pane layout, URL state, save semantics; new surfaces
  supply the form fields and the usage query.
- **Destructive actions stay loud.** Modals retain their
  intended attention-grabbing role for delete confirmations
  rather than being misused for routine create / edit.

## Adoption

Five current vocabulary surfaces:

- CodeSystem, ValueSet, ConceptMap, NotationPattern (settings),
- Glossary terms (workbench),
- Mappings (workbench),
- Rule editing (settings — uses `RuleForm` inside the editor
  pane).

The next CRUD surface (concept editor when ADR-0014 stage 2
lands, action authoring when Phase 5 ActionExecutor lands) is
expected to drop into `MasterDetailEntityPage` directly.

## Alternatives considered

- **Modal-only flows** — rejected. The three failure modes
  (no comparison, no usage, no deep-link) drove the migration.
- **Drawer / side-sheet** — rejected. Drawers occlude the
  list partially but still hide the "what surrounds this" axis;
  the pane-split pattern keeps both visible.
- **Inline-row editing (Airtable-style)** — rejected. Form
  fields don't fit cleanly inside a row when the entity has
  10+ properties; the editor pane gives breathing room.

## References

- Linear settings, Stripe Dashboard, Notion database, Sanity
  Studio (industry pattern)
- Memory entry: `feedback_master_detail_over_modal.md`
- Memory entry: `feedback_form_section_chrome_lightweight.md`
  (the matching form-chrome decision: fieldset+legend, not
  card chrome)
- Primitives: `web/src/components/vocabulary/master-detail-entity-page.tsx`,
  `web/src/components/workbench/entity-workbench.tsx`,
  `web/src/components/forms/save-bar.tsx`,
  `web/src/components/forms/form-section.tsx`
