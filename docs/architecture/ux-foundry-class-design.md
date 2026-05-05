# UX Foundry-class — Cypher autocomplete + multi-tab result + canonical canvas

**Status:** Design sketch — Phase 9 of the long-horizon
work plan. The eight UX gaps the revised plan highlighted
each carry independent shippability; this document captures
the contract for the four single-largest-leverage items so
the next session has the design + integration points + test
plan in one place. The other four (saved-scene v2,
investigation/case route, timeline scrubber, geospatial)
stay deferred until the use case crosses the volume bar.

## Volume / use-case gates

Per the revised work plan, four UX items defer until a
real use case arrives:

- **Saved-Scene v2** — until per-workspace
  perspective-usage telemetry shows operators routinely
  saving + reloading view state.
- **Investigation / case route** — until a case-management
  workflow lands on the product roadmap.
- **Timeline scrubber on canvas** — until temporal
  bitemporal queries become a common path.
- **Geospatial canvas overlay** — until `geo_location`
  property usage crosses a threshold workspace count.

The four below ship without a volume gate — they fix
documented analyst-grade UX gaps regardless of workspace
shape.

## Decision (sketch) — four shippable pieces

### 1. Cypher autocomplete grounded in `useWorkspaceOntology()`

The single highest-leverage analyst UX win. `code-editor-inner.tsx`
ships a `StreamLanguage` Cypher tokenizer for syntax
highlighting, but no autocomplete extension — analysts
authoring Cypher in the canvas / analyze panels get no
schema-aware completion.

Add `@codemirror/autocomplete` extension wired to the
workspace's active ontology:

```tsx
import { autocompletion, CompletionContext, CompletionResult } from "@codemirror/autocomplete";
import { useWorkspaceOntology } from "@/hooks/api/use-workspace-ontology";

function cypherCompletionFromOntology(ontology: OntologyDetail) {
  return (context: CompletionContext): CompletionResult | null => {
    const before = context.matchBefore(/[a-zA-Z_가-힣]+/);
    if (!before || before.from === before.to && !context.explicit) return null;

    const word = before.text;
    const tokenContext = inferTokenContext(context); // node label / rel type / property?

    const options = match(tokenContext, {
      NodeLabel: () => ontology.node_types.map((nt) => ({
        label: nt.label,
        type: "class",
        detail: localize(nt.display_name) || nt.label,
        info: localize(nt.description),
      })),
      RelType: () => ontology.edge_types.map((et) => ({ label: et.label, type: "interface", ... })),
      Property: (label) => ontology.node_types
        .find((nt) => nt.label === label)
        ?.properties.map((p) => ({ label: p.name, type: "property", detail: p.semantic_type, ... }))
        ?? [],
      Keyword: () => CYPHER_KEYWORDS.map((k) => ({ label: k, type: "keyword" })),
    });

    return { from: before.from, options };
  };
}
```

Token context inference (`inferTokenContext`) inspects the
preceding tokens — `(:` or `:` after `MATCH` / `MERGE` /
`CREATE` → `NodeLabel`; `[:` or `]-` → `RelType`; `.` after
a bound variable → resolve the variable's label and offer
`Property` completions; otherwise `Keyword`.

Korean labels work naturally — the regex captures Hangul
codepoints; the completion list matches against
`localize(display_name)` so analysts type `고객` and the
completion offers `Customer` (or whichever GraphLabel the
operator authored).

### 2. Multi-tab result panel (Graph / Table / JSON / Cypher)

`analyze-results-panel.tsx` today renders one result view
at a time. Every other graph platform (Neo4j Browser, Bloom,
Stardog Studio) ships the four-tab result panel — the
analyst flips between visualisations without re-running the
query.

Adopt the four canonical tabs:

- **Graph** — node-edge force layout via
  `@xyflow/react` (the unified graph engine, per the
  Phase 9 plan that retires `react-force-graph-2d`).
- **Table** — `@tanstack/react-virtual`-backed virtualised
  rows over the result rows. Wide tables get
  `min-w-[900px]`.
- **JSON** — collapsible JSON tree of the raw result rows
  (the streamdown JSON renderer reused).
- **Cypher** — the compiled Cypher with syntax highlighting
  + a "copy" affordance. Opens the editor pre-populated for
  the analyst to tweak + re-run.

Tab state lives in URL hash (`#tab=graph`) so deep-links
land on the analyst's preferred view. The default tab is
`graph` for results with non-trivial edge count, `table`
otherwise (heuristic on the result metadata).

### 3. In-canvas search popover (Bloom-class)

Today the `search-dialog.tsx` is a global navigation dialog
— the canvas itself has no "find this node and pan to it"
affordance. Bloom and Linkurious both keep an inline canvas
search as their default discovery surface.

Add `<CanvasSearch>` rendered inside the canvas viewport
(top-right corner, hotkey `/`):

```tsx
<CanvasSearch
  visibleNodes={visibleGraphNodes}
  onSelect={(node) => {
    centerOnNode(node);                // pan + zoom
    flashHighlight(node, { duration: 1200 });
    setSelected([node.id]);
  }}
/>
```

The popover renders a textbox + a fuzzy-matched list of
visible nodes (label / display name / id). Selecting a
node pans the viewport to centre it + flashes a 1.2s
highlight + sets it as the selection. Keyboard nav
(arrow / enter) and `Esc` to dismiss.

### 4. Drop `react-force-graph-2d`, unify on `@xyflow/react`

Two graph engines for two surfaces (`graph-widget.tsx`
uses `react-force-graph-2d`; every canvas / explore /
query-builder surface uses `@xyflow/react`) means two
mental models, two pan-zoom-select grammars, ~120kb of
extra chunk weight, and divergent interaction
ergonomics.

Migrate `dashboard/widgets/graph-widget.tsx` to
`@xyflow/react` with the `forceCollide` /
`forceManyBody` ELK preset configured for force-layout
rendering. The unified canvas grammar covers the remaining
use case without a second library; the chunk-weight
saving falls through to the dashboard widget's bundle.

Verification:

- Visual smoke test on `dashboard/widgets/graph-widget.tsx`
  in the existing dashboard fixtures — the layout should
  look essentially identical (force-directed) under the
  ELK preset.
- `react-force-graph-2d` removed from
  `web/package.json` after the cutover; `pnpm install`
  drops the transitive `d3-force` chain.

## Action surface (Phase 5 dependency)

The matching FE work for ActionExecutor (per the
`action-executor-design.md` sketch) ships in Phase 5;
this Phase 9 surface composes with it:

- The multi-tab result panel grows an "Actions" tab when
  the result rows match an `ActionDef`'s `subject`
  (operator-driven type binding).
- The bulk-action-bar primitive (per ADR-0020) gains an
  `actionRegistry` slot for cohort invocation.
- `<ApprovalGate proposalId={...} onApproved={...}>` wraps
  inline action buttons so `RequiresApproval` routing
  surfaces the inline approval flow without leaving the
  workbench.

## Integration points

- **No changes to existing FE state** — Cypher
  autocomplete reads `useWorkspaceOntology()`; tab state
  is URL hash; canvas search is local component state.
- **CSS / token surface** stays on the existing design
  system (per ADR-0026); no new tokens required.
- **`pnpm gate` parity** — the new components add
  tests to the existing vitest surface; the
  `heading-primitive-audit` and `design-rules-audit`
  gates already cover them.

## Test pyramid

- **Vitest** on `cypherCompletionFromOntology` —
  inferred-context decision table over the canonical
  Cypher prefixes.
- **Vitest-axe** on `<CanvasSearch>` for keyboard
  navigation + focus management.
- **Playwright** on the multi-tab result panel —
  `?tab=...` deep link round-trip + tab keyboard
  navigation.
- **Bundle-size assertion** in CI after the
  `react-force-graph-2d` removal — the dashboard chunk
  drops by the expected ~120kb.

## Out of scope (deferred)

Per the volume / use-case gates above:

- **Saved-Scene v2** — perspective state extension
  (camera + filters + facet config + timeline cursor).
- **Investigation / case route** — Linkurious-class
  case management workflow.
- **Timeline scrubber pinned to canvas** — bitemporal
  data exploration UI.
- **Geospatial canvas mode** — Leaflet / MapLibre
  overlay for `geo_location` annotated entities.

Each defers to a future iteration when the use case
crosses the volume threshold.

## References

- ADR-0020 — `BulkActionBar` (the cohort surface the
  new "Actions" tab composes with).
- ADR-0021 — Master-detail over modal (the result-tab
  pattern's UX heritage).
- ADR-0022 — `PluginRegistry<T>` (the
  actionRegistry that the Actions tab reads).
- ADR-0026 — Design system Φ1–Φ7 (the token + primitive
  layer this surface stays consistent with).
- `docs/architecture/action-executor-design.md` —
  Phase 5 second half (the matching ActionExecutor
  surface).
- `docs/architecture/plan-router-design.md` — Phase 6
  (the routing decision surfaced in the result panel's
  metadata).
- Memory entry: `feedback_master_detail_over_modal.md`,
  `feedback_bulk_action_bar_primitive.md`,
  `feedback_optimistic_mutation_hook.md`.
- Phase 9 of the long-horizon plan (revised).
- Industry references: Neo4j Browser, Bloom, Stardog
  Studio, Foundry Workshop.
