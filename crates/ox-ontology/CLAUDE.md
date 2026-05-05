# ox-ontology

Domain layer for the platform's knowledge graph: what types of nodes
and edges exist, how they're labelled, which properties they carry,
how they map to physical relations, and every governance surface
layered on top (interfaces, rules, provenance, data quality,
glossary, functions, actions, metrics, enrichment).

This is the biggest IR crate by surface area. The doc below names
the high-level pieces and points at the files that hold depth.

## Core IR

- **`OntologyIR`** (`src/ir/mod.rs`) — the root struct. Owns the full
  graph schema plus every governance collection:
  - `node_types`, `edge_types` — primary topology.
  - `indexes`, `constraints` — lookup + invariant metadata.
  - `object_mappings`, `link_mappings`, `property_mappings`
    — physical mapping layer. See `src/mapping/`.
  - `interfaces` — shared-property abstractions (e.g., `HasAddress`).
  - `rules` — SHACL-style constraints.
  - `actions`, `functions`, `metrics`, `enrichments` —
    type-bound behavioural surfaces.
  - `glossary_terms` — domain vocabulary; attaches to types.
  - `provenances` — PROV-O style data-origin records.
  - `data_qualities` — per-type DQ checks.
  - `lineage_id` + `version`, optional `valid_from`/`valid_to`
    — bitemporal identity.

  Every collection has an `add_X` method returning
  `Result<_, OntologyInvariantError>` (referential integrity check
  at insert time) and an `x_by_id` O(1) accessor.

- **`OntologyIR::validate()`** (`src/ir/validation.rs`) — whole-IR
  cross-reference check. Call at ontology-edit boundaries; returns
  `Vec<String>` of diagnostic strings (empty on valid).

## Mapping layer

`src/mapping/`:

- `ObjectMappingDef` — one NodeType ↔ one physical relation.
  Carries `workspace_scope`, `row_filter`, `primary_key_columns`,
  `valid_from`/`valid_to` (temporal pivot support), `precedence`
  (for multi-mapping dedup), and `cache_hint`.
- `LinkMappingDef` — one EdgeType ↔ the relation(s) supplying edges.
  Four `LinkMappingKind` variants:
  - `ForeignKey { source_column, target_column }`
  - `Bridge { bridge_relation, source_join: Vec<ColumnRef>,
    target_join: Vec<ColumnRef> }` — composite keys are supported.
  - `Computed { predicate }` — source-dialect SQL predicate;
    needs adapter-side pushdown.
  - `Federated { source_match_column, target_match_column }` —
    cross-source value match.
- `PropertyMappingDef` — one property ↔ one value location (column /
  JSON path) plus optional `PropertyTransform`.
- `SourceId`, `ObjectMappingId`, `LinkMappingId` — id newtypes.
- `SourceRelationRef`, `ColumnRef`, `EndpointRef` — location
  primitives.

`OntologyIR.object_mappings` is the single source of truth for
node ↔ table binding. The legacy flat-HashMap `SourceMapping` and
the transitional `ObjectMappingLookup` trait have been removed;
all PII / quality / load-plan consumers walk the canonical slice
directly.

## Governance surfaces

Each has its own file and follows the same structural pattern:
id newtype + struct + builder + validation entry-points.

- `interface.rs` — `InterfaceDef { required_properties,
  required_edges }`. Matched by `LabelResolver` in ox-federation.
- `rule.rs` — SHACL Core rule kinds (pre-execute validation).
- `action.rs`, `function.rs`, `metric.rs`, `enrichment.rs` —
  type-bound behaviours; scheduled / triggered from ox-api.
- `glossary.rs` — domain vocabulary.
- `provenance.rs` — PROV-O activity/entity/agent.
- `data_quality.rs` — assertion + severity + threshold.

## Input / command / quality sub-modules

- `src/input/` — DTO layer for user / LLM input before validation.
  `InputXxxDef` structs convert to their canonical `XxxDef` counterparts.
- `src/command/` — `OntologyCommand` — incremental schema edits
  (add / delete / rename node / edge / property). Reconciled into
  the authoritative `OntologyIR`.
- `src/quality/` — ontology-level quality assessment (different from
  `data_quality.rs` which is per-node-type DQ).

## Analysis / scratch surfaces

- `audit.rs`, `diff.rs` — before/after reports for edits.
- `insight.rs`, `repo_insights.rs`, `source_analysis.rs` — design-time
  recommendations.
- `graph_exploration.rs`, `table_clustering.rs`,
  `widget_spec.rs`, `load_plan.rs`, `design_project.rs` — per-surface
  DTOs the ox-api routes return. These do not roll into `OntologyIR`;
  they describe projects / plans / explorations over it.

## IR invariants enforced by `validate()`

These are the platform-wide rules every persisted ontology must
satisfy. The validator emits structured `DiagnosticMessage`s
(stable code + params) the FE renders through next-intl; CI
fails any test fixture or migration that violates them.

- **Mapping carries meaning.** A `PropertyDef` with `source_column`
  set must have ≥1 `PropertyBinding` *or* a `binding_exempt`
  reason (`PrimaryKey`, `AuditTimestamp`, `OpaqueIdentifier`,
  `Custom(_)`). `aggregation_role = Identifier` is an implicit
  exemption. Diagnostic:
  `ontology.validate.property.mapping_without_binding`.
- **Composition keeps source singular.** `EdgeKind::Composition`
  requires `cardinality.source_is_singular()` (`OneToOne` /
  `OneToMany`). UML strong ownership = each part has exactly one
  whole; `ManyToOne` / `ManyToMany` would break cascade-delete.
- **Derived rules track their source.** A `RuleDef` with
  `RuleOrigin::DerivedFromBinding { node, property }` must point at
  a property that still carries ≥1 binding. Unbinding the source
  forces the rule to be regenerated or promoted to `Authored`.
- **Glossary anchors resolve.** Every `GlossaryTermId` referenced
  from `NodeTypeDef.glossary_anchors` /
  `EdgeTypeDef.glossary_anchors` /
  `PropertyBinding::Glossary { id }` must exist in
  `OntologyIR::glossary`.

## Binding resolution is deterministic

When several `PropertyBinding`s share a kind, the canonical pick
is the highest `BindingStrength::priority`
(`Required`(4) > `Preferred`(3) > `Extensible`(2) > `Example`(1)),
ties broken by first-in-list. Insertion-order shuffles that don't
change the strength distribution don't change the answer.
`PropertyDef::value_set_binding()` /
`notation_pattern_binding()` / `glossary_binding()` etc. all route
through `canonical_binding()` so consumers cannot accidentally
reach the lower-priority entry.

## IR JSONB schema evolution

`OntologyIR` is persisted as JSONB. Every row carries
`schema_version`; `OntologyIR::deserialize` runs every read through
the migration pipeline at `src/ir/migration.rs::migrate_to_current`
so older rows transparently load on newer builds.

When bumping `ONTOLOGY_IR_SCHEMA_VERSION` from N to N+1, classify
the change:

- **Additive only** (new `Vec<...> #[serde(default)]` collection,
  fields stay the same): no migration needed. `serde(default)`
  populates absent fields; the pipeline just walks the version
  tag forward.
- **Structural** (rename, fold-in, restructure): write
  `ir/migration/v{N}_to_v{N+1}.rs` implementing `IrMigration` and
  register it in `migration.rs::migrations()`. The chain test
  `migration_chain_is_continuous` fails the build if a step
  is missing or skips versions; `each_migration_advances_by_one`
  pins the +1 advancement.

Every structural migration ships two fixture tests at minimum: a
pre-image v{N} JSON blob round-trips through `migrate_to_current`
to the expected v{N+1} shape, and a dangling-reference / edge case
test pins the cleanup behaviour. See `v4_to_v5.rs` (concept
fold-in) for the canonical template.

## Don't

- Don't add an `extends` / `parent` / `super_type` field to
  `NodeTypeDef`. Type taxonomy is `InterfaceDef.implements: Vec<InterfaceId>`
  — a node lists the interfaces it fulfils and the federation
  planner's `InterfaceExpander` resolves `(:Iface)` to the union
  of concrete implementers. Forcing a single named superclass
  loses multiple inheritance, drags display semantics into the
  query path, and competes with the canonical primitive.
- Don't mutate `OntologyIR` collections directly — go through the
  `add_X` methods so referential-integrity checks run at insert.
- Don't introduce a reverse edge from `ox-core` to this crate. The
  workspace DAG keeps `ox-core ← ox-ontology` strict; `cargo-deny`
  enforces it.
- Don't mix the "canonical `XxxDef`" structs with the
  `InputXxxDef` structs across module boundaries. Input DTOs carry
  pre-validation shape; canonical defs come out of validation. The
  compiler / runtime / federation layers only ever see canonical.
- Don't add raw SQL / Cypher strings here. This crate stays logical —
  physical translation lives in `ox-compiler` (Cypher) and
  `ox-federation` (DataFusion LogicalPlan).
