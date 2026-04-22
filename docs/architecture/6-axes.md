# The Six Semantic Axes

The Ontosyx platform identity is defined by six axes. Every feature
maps to one or more of them. A proposed feature that does not map to
any axis is out of scope for Ontosyx — we do it somewhere else, or we
do not do it at all.

The axes are normative, not descriptive: they pre-commit the platform
to a shape, so that individual design decisions can be checked against
the whole rather than re-argued each time.

## S1 — Semantics

*Concepts, relations, rules, derivations, measures are first-class.*

What counts as semantic:
- `NodeTypeDef`, `EdgeTypeDef`, `InterfaceDef`
- `PropertyDef` with `semantic_type`, `classification`, `pii_kind`,
  `glossary_term_id`, `aliases`, `business_context`
- `RuleDef` (SHACL Core) for constraints
- `FunctionDef` for derivations
- `ActionDef` for state-mutating operations
- `MetricDef` for aggregate KPIs
- `EnrichmentDef` for external data joins
- `GlossaryTermDef`, `TaxonomyDef`

Invariants:
- An ontology is never only a schema. If a concept cannot be described
  semantically, we model it as `FunctionDef` or `EnrichmentDef`, never
  as a code comment.
- Every semantic concept has an IRI (ADR 0010) and is exportable as
  RDF/Turtle.

## S2 — Physical Independence

*Logical ontology is decoupled from physical storage through first-class
Mappings.*

- `ObjectMappingDef`, `LinkMappingDef`, `PropertyMappingDef`
- Multi-mapping union with `precedence`
- Row-filter / workspace-scope pushdown
- Source capability introspection drives planner decisions

Invariants:
- A node type has **zero or more** physical mappings; zero is the
  "logical-only" case (e.g., a `DerivedProperty`-only type).
- No executable plan fragment accesses a source except through a
  mapping.

## S3 — Execution

*Queries execute over original sources by default (VOL). Graph cache is
opt-in per mapping.*

- Apache DataFusion federation engine (ADR 0002)
- `GraphCacheBackend` trait (ADR 0004)
- Capability-aware planner: rewrite, route, or reject — never silent
  fallback

Invariants:
- No migration required to use Ontosyx.
- The fastest possible query for a customer is their current DB tuned
  with their current indexes; we add a semantic layer, not a bottleneck.

## S4 — Time

*Ontology time and data time are independent axes (bitemporal).*

- `QueryIR.ontology_valid_at` — which ontology version to interpret
- `QueryIR.data_valid_at` — which source data snapshot to read
- `ObjectMappingDef.valid_from / valid_to` — when a mapping was alive
- `ProvenanceDef.at_time` — when the fact was asserted

Invariants:
- "Today's model, last quarter's data" is a valid query shape.
- Rename / split / merge of concepts never silently rewrites old
  queries; `ontology_valid_at` preserves the old lexicon.

## S5 — Provenance & Quality

*Every fact carries origin and quality. Both are first-class.*

- `ProvenanceDef` (PROV-O aligned)
- `DataQualityDef` (5 dimensions: completeness, validity, uniqueness,
  consistency, timeliness, accuracy)
- `AuditEventDef` (append-only, workspace-scoped)

Invariants:
- No result leaves the system without an attributable origin.
- Data quality is measured, not assumed; scores are visible to the
  consumer.

## S6 — Operations

*Drift, change, approval, cost, and audit are first-class.*

- `SchemaDriftDef` with `Detected → Acknowledged → Resolved | Ignored`
- `ActionDef.approval: ApprovalPolicy`, `idempotency: IdempotencyPolicy`
- `CostTierKind` per source; `CostEstimator` fronts the planner
- Prompt / mapping / rule hot-reload

Invariants:
- Nothing that mutates state escapes an approval gate unless explicitly
  opt-in at the workspace-governance level.
- A source schema change is an inbox item, not a silent mapping drift.
- Operational cost is visible to the user before it is charged.

## Using the axes

For every PR:

1. Which axes does this PR touch?
2. Does it preserve the invariants on those axes?
3. If it introduces a new first-class concept, which axis owns it?
4. If it is a cross-axis change, is the coordination explicit (ADR
   or linked issue)?

The axes are the standing invariants; ADRs are the changes to those
invariants.
