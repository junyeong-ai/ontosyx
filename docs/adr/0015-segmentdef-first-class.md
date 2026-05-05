# 0015 — `SegmentDef` as a first-class IR collection

**Status:** Accepted

**Date:** 2026-05-01

**Supersedes:** none

## Context

`ConceptDef::realisation` (per ADR-0014) carries one of three
variants: `Segment`, `Function`, or `CrossEntity`. Of those,
`Segment` is the most common authoring shape — operators
articulate "Active Customer = Customer whose `last_order_at` is
within 90 days" as a saved pattern over the existing schema.

The pre-Segment design embedded the saved pattern inline on the
concept (or earlier, on the glossary term). Two costs followed:

- **Duplication.** Two concepts that referenced the same filter
  ("Active Customer" and "Recently Active Account" both bound
  on `last_order_at < 90d`) duplicated the predicate JSON; an
  edit to the rule had to be replayed everywhere.
- **No reuse from queries.** A `MATCH (c:Customer)` that wanted
  the same filter had to either re-author the predicate or
  resolve through the concept indirectly — the planner had no
  way to recognise "this is the same saved segment".

Foundry's "Saved Search" / "Object Set", Stardog's "View",
TopBraid's "SPARQL ASK rule" all keep the saved-filter as a
first-class shareable entity. The metamodel needs the same to
make `Concept.realisation: Segment` cheap to author and reuse.

## Decision

`SegmentDef` becomes a first-class IR collection
(`OntologyIR.segments`) with the contract:

- **Stable id** — `SegmentId` newtype.
- **Body is a `PatternIR` slice** — the same UX-grade pattern
  language `QueryIR` lowers from. The compiler's pattern-rewrite
  pass already understands it; segment bodies don't reinvent
  predicate syntax.
- **Bound endpoint shape** — `SegmentDef.input_kind:
  EntityKind` declares whether the body filters nodes
  (`EntityKind::Node { node_type_id }`), edges, or arbitrary
  pattern matches. Concepts bind on the matching shape so
  `Concept.realisation: Segment` against a NodeType-segment
  membership-tests a node.
- **Reusable across surfaces** — query writers reference
  `:Segment(s-active-customer)` as a syntactic shortcut for
  "expand the saved segment here"; the compiler inlines the
  pattern body at the call site. Functions and metrics reach
  the same way (`MetricExpression::Count {
  scope: SegmentRef(s-active-customer) }`).
- **Versioned alongside the ontology** — segments live inside
  `OntologyIR` so they roll into the immutable version
  snapshot pipeline. A query frozen against snapshot V uses
  the segment as it was at V; renaming a segment in V+1
  doesn't retroactively change V's results.

## Realisation chain

When the runtime resolves a concept's membership:

```
Concept c-active-customer
  → realisation: Segment { segment_id: s-active-customer }
  → SegmentDef { body: PatternIR(...) }
  → compiler: PatternIR → QueryIR → Cypher predicate
```

Three indirections, each replaceable independently:

- swap a concept's realisation between Segment / Function /
  CrossEntity without touching the segment body
- edit the segment body (and re-version the ontology) without
  touching every concept that references it
- evolve the compiler's PatternIR lowering without touching the
  authored segments

## Consequences

- **Concept editor** — the concept admin UI offers a "pick
  segment" picker rather than an inline pattern editor.
  Authoring a concept becomes a one-click bind to an existing
  segment in the common case.
- **NL→Cypher prompt budget** — schema-RAG renders saved
  segments as compact callable units in the prompt header. The
  LLM sees "`:Segment(s-active-customer)`" instead of an
  inlined predicate, which fits more concepts in the same token
  budget and produces cleaner generated queries.
- **Validation cost** — adding a segment runs the same
  `OntologyIR::validate()` pass; the body's referenced
  NodeTypes / EdgeTypes / properties must resolve, identical
  to a regular pattern. No new validator surface.
- **Federation** — segment bodies that touch federated sources
  inherit the planner's federation path automatically; segments
  are sugar over `PatternIR`, not a parallel runtime.

## Alternatives considered

- **Inline pattern on `ConceptDef`** — rejected. Duplicates
  predicates across concepts; loses reuse from queries; forces
  every concept editor to render a full pattern UI.
- **Segment as a query-side macro only** — rejected. Without
  versioning the segment lives outside the ontology snapshot
  and a query frozen at V can't reproducibly resolve it under
  a later edit.
- **Segment owns its own collection in `ox-store` (not in
  `OntologyIR`)** — rejected. The version snapshot pipeline
  becomes ambiguous; "what version of segment X did this
  query resolve against" requires a parallel temporal index.

## References

- ADR-0014 — `ConceptDef` as canonical identity
- Memory entry: `feedback_concept_def_first_class.md`
- Foundry Saved Search docs (Palantir public blog)
