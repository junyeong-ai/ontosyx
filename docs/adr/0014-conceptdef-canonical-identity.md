# 0014 — `ConceptDef` as canonical identity above `NodeTypeDef`

**Status:** Accepted

**Date:** 2026-05-01

**Supersedes:** none — `ConceptDef` is a new layer; the prior model
identified concepts implicitly through `GlossaryTermDef.term` strings
and the `NodeTypeDef.glossary_anchors` back-reference.

## Context

The previous metamodel collapsed three things into one — the *concept*
("Customer"), its *lexicalisation* ("고객" / "Customer" / "client"
synonyms), and its *implementation* (`NodeTypeDef` rows in the graph
schema). When two systems both modelled "Customer" but the operator
wanted federation to recognise them as **the same concept**, the only
hook was a string match on the glossary term, which:

- Broke under multilingual deployments — the Korean and English
  spellings were distinct strings even though they meant the same
  thing.
- Drifted under reorganisations — renaming `t-customer` to
  `t-buyer` silently severed every cross-source link unless every
  caller updated in lockstep.
- Conflated identity with display — the LLM prompt path saw
  `prefLabel` strings instead of stable ids, so a paraphrase in
  the prompt could land a "matching" link on the wrong term.

Foundry, Stardog, TopBraid, PoolParty all treat the concept as a
distinct identity layer above the lexicalisation. SKOS itself
specifies it: `skos:Concept` (the identity), `skosxl:Label` (the
lexicalisation). ISO 1087 follows the same split (concept ↔ term).
Aligning with the validated standards forced the same separation
into our IR.

## Decision

`ConceptDef` becomes a first-class IR collection (`OntologyIR.concepts`)
with the canonical identity contract:

- **Stable id** — `ConceptId` newtype, never the lexicalisation
  string. Renaming a term doesn't break the concept link.
- **1:1 anchor with `GlossaryTermDef.canonical_term_id`** — every
  concept names exactly one term as its preferred lexicalisation;
  every other term that renders the concept lives on the term
  itself (`GlossaryTermDef.concept_id`) as an alt-label / hidden
  label entry.
- **Executable realisation** — `ConceptDef.realisation:
  TermRealisation` carries one of `{Segment | Function |
  CrossEntity}`. The runtime resolves "is this row a member of
  concept X" through the realisation, not through string matches.
- **Reverse index** — `OntologyIR.concept_realised_by_node_types`
  maps a concept to every NodeType that implements it. Federation
  walks this index when a query names the concept rather than a
  specific NodeType.
- **Wire-bound on `NodeTypeDef.concept_id` /
  `EdgeTypeDef.concept_id` /
  `PropertyBinding::Concept { id: ConceptId }`** — every
  schema-element ↔ concept link goes through the typed id, not a
  string.

## TermRealisation variants

`TermRealisation` is a closed enum:

```rust
pub enum TermRealisation {
    Segment    { segment_id: SegmentId },        // saved-pattern
    Function   { function_id: FunctionId },      // computed
    CrossEntity { predicate: String },           // structured filter
}
```

`Query` (saved-view realisation) was rejected because `InsightId`
lives in `ox-store` and a forward dependency
`ox-ontology → ox-store` would invert the workspace DAG enforced
by `cargo-deny`. View-shaped concepts use `Function` whose body
returns the saved-view rows.

## Consequences

- **NL→Cypher** — the LLM prompt path sees concept ids instead of
  English glossary strings. A paraphrase in the question still
  lands on the same concept because the schema-RAG layer
  resolves to `ConceptId` before prompt rendering.
- **Federation** — `(:Customer)` in user-facing query syntax
  expands to every NodeType whose `concept_id == c-customer` via
  the reverse index. The cross-source "this is the same Customer"
  guarantee that gives federation its value depends on this
  expansion.
- **SKOS export** — `ConceptDef` round-trips to `skos:Concept`,
  the realisation drops to `skos:scopeNote`, the canonical term
  drops to `skos:prefLabel`, alt terms to `skos:altLabel`. No
  semantic loss across the export boundary.
- **`NodeTypeDef.glossary_anchors` deprecated** — the field
  predates `ConceptDef`. Stage 2 of this ADR (deferred at
  authorship time, see "Open follow-ups" below) collapses
  `glossary_anchors` into `concept_id`; a node's lexicalisation
  reaches via `concept → canonical_term_id` instead of a direct
  pointer. Until that lands the validator accepts both pointers
  but warns on divergence.
- **`PropertyBinding::Glossary { id: GlossaryTermId }` deprecated**
  — same axis. The binding should reach via the concept layer;
  `PropertyBinding::Concept { id: ConceptId }` is the new
  canonical variant.
- **HeuristicProposal-style queue** — concept proposals from the
  LLM never write directly to `OntologyIR.concepts`. They land in
  the proposal queue (per ADR-0023) and require operator approval
  before promotion, mirroring the "no automatic decisions"
  invariant.

## Alternatives considered

- **Keep glossary terms as identity** — rejected. The string-keyed
  identity model is brittle under multilingual deployments and
  renames; every validated metamodel platform splits the layers.
- **Inline realisation on every NodeType** — rejected. Two
  NodeTypes both realising "Active Customer" would duplicate the
  segment definition; the duplication drifts on edit and the
  federation planner can't reason about "same concept" by
  identity.
- **One concept per term (no canonical anchor)** — rejected.
  Loses the SKOS-canonical mapping ("everyone reads `prefLabel`
  for the rendering, walks `altLabel`s for synonym matching")
  that downstream tools (export, NL alias resolution) depend on.

## Open follow-ups

Stage 2 of this ADR — collapse `NodeTypeDef.glossary_anchors`
into `concept_id` and replace `PropertyBinding::Glossary` with
`PropertyBinding::Concept` — was deferred at authorship to keep
this slice landing-sized. The double-bookkeeping is documented as
debt; the matching cleanup commit retires the deprecated fields
once every consumer reaches via `concept → canonical_term_id`.

## References

- W3C SKOS Core — <https://www.w3.org/TR/skos-reference/>
- ISO 1087 — Terminology work — Vocabulary
- FIBO — <https://spec.edmcouncil.org/fibo/>
- Memory entry: `feedback_concept_def_first_class.md`
- Memory entry: `feedback_canonical_korean_term.md`
