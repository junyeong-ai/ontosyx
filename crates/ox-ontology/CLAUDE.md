# ox-ontology

Domain layer of the platform's knowledge graph: node / edge types, mappings to physical relations, and every governance surface layered on top (interfaces, rules, glossary, provenance, data quality, functions, actions, metrics, enrichments).

`OntologyIR` (`src/ir/mod.rs`) is the root struct. Each governance collection has an `add_X` method that runs referential-integrity at insert time and returns `Result<_, OntologyInvariantError>`, plus an O(1) `x_by_id` accessor. Don't mutate the collection vectors directly — go through the `add_X` methods.

`OntologyIR::validate() -> Vec<DiagnosticMessage>` runs whole-IR cross-reference validation (empty vec on valid). Diagnostics carry stable `code` + `params`; the FE i18n catalogue interpolates the prose.

## Identifier conventions

- **Canonical defs** (`XxxDef`): the validated, persisted shape. The compiler / runtime / federation only ever see canonical.
- **Input DTOs** (`InputXxxDef`, `src/input/`): the user / LLM shape *before* validation. Convert to canonical at the validation boundary.
- **Commands** (`OntologyCommand`, `src/command/`): incremental schema edits (add / delete / rename) reconciled into the canonical `OntologyIR`.

## Mapping layer

`src/mapping/` binds logical types to physical relations:

- `ObjectMappingDef` — one NodeType ↔ one relation. Carries `workspace_scope`, `row_filter`, `primary_key_columns`, `valid_from`/`valid_to` (temporal), `precedence` (multi-mapping dedup), `cache_hint`.
- `LinkMappingDef` — one EdgeType ↔ relation(s). Variants: `ForeignKey`, `Bridge` (composite keys supported), `Computed` (source-dialect SQL predicate, needs adapter pushdown), `Federated`.
- `PropertyMappingDef` — one property ↔ one column / JSON path with optional `PropertyTransform`.

`OntologyIR.object_mappings` is the only source of truth for node-to-table binding. PII / quality / load-plan consumers walk that slice directly.

## IR invariants enforced by `validate()`

The validator emits structured `DiagnosticMessage`s; CI fails any fixture or migration that violates these:

- **Mapping carries meaning.** A `PropertyDef` with `source_column` set must have ≥1 `PropertyBinding` *or* a `binding_exempt` reason (`PrimaryKey`, `AuditTimestamp`, `OpaqueIdentifier`, `Custom(_)`). `aggregation_role = Identifier` is an implicit exemption.
- **Composition keeps source singular.** `EdgeKind::Composition` requires `cardinality.source_is_singular()` (`OneToOne` / `OneToMany`). `ManyToOne` / `ManyToMany` would break cascade-delete (UML strong ownership = each part has exactly one whole).
- **Derived rules track their source.** A `RuleDef` with `RuleOrigin::DerivedFromBinding { node, property }` must point at a property that still carries ≥1 binding.
- **Concept bindings resolve.** Every `ConceptId` referenced from `NodeTypeDef.concept_id`, `EdgeTypeDef.concept_id`, graph-type `concept_realizations`, or `PropertyBinding::Concept { id }` must exist in `OntologyIR::concepts`.

## Binding resolution is deterministic

When several `PropertyBinding`s share a kind, the canonical pick is the highest `BindingStrength::priority` (`Required`(4) > `Preferred`(3) > `Extensible`(2) > `Example`(1)), ties broken by first-in-list. `PropertyDef::value_set_binding()` / `notation_pattern_binding()` / `concept_binding()` route through `canonical_binding()` — consumers cannot accidentally reach a lower-priority entry.

## Implicit-rule derivation — two axes

`derived_rules.rs` synthesises SHACL rules so a write reaching the runtime validator gets every schema-level invariant enforced without an operator hand-authoring a redundant `RuleDef`:

- **Binding axis** — `PropertyBinding { strength: Required }` → `InValueSet` / `MatchesPattern`. CodeSystem-targeted bindings produce nothing — wrap in a value set if you need enforcement.
- **Nullable axis** — `PropertyDef.nullable=false` → `MinCount=1`.

`derive_implicit_rules()` is the union the SHACL validator and the dedup index call. Per-axis variants (`derive_binding_rules` / `derive_nullable_rules`) exist for consumers that filter to one axis.

Dedup is signature-keyed via `ConstraintSignature`. New constraint kinds opt in by returning `Some(...)` from `ShaclConstraint::signature()`; `None`-signed constraints never collapse. `MinCount`'s signature ignores the numeric `min`, so an authored `MinCount=2` correctly suppresses the implicit nullable derivation.

## On-wire shape gate

Every IR carries an explicit `schema_version` constant (`ONTOLOGY_IR_SCHEMA_VERSION`, `QUERY_IR_SCHEMA_VERSION`, etc.). Deserialisation rejects a JSONB row whose version exceeds the running build's — fail-fast on a future-shape blob. Additive optional fields ride on `#[serde(default)]` and don't require a bump; an incompatible struct-shape change forces a coordinated re-materialise across deployments.

## Don't

- Don't add an `extends` / `parent` / `super_type` field to `NodeTypeDef`. Type taxonomy is `InterfaceDef.implements: Vec<InterfaceId>`; the federation planner's `InterfaceExpander` resolves `(:Iface)` to the union of concrete implementers.
- Don't mutate `OntologyIR` collections directly — use the `add_X` methods so referential-integrity checks run.
- Don't introduce a reverse edge from `ox-core` to this crate. The workspace DAG keeps `ox-core ← ox-ontology` strict; `cargo-deny` enforces it.
- Don't mix canonical `XxxDef` with `InputXxxDef` across module boundaries. Compiler / runtime / federation only see canonical.
- Don't add raw SQL / Cypher strings here. Physical translation lives in `ox-compiler` (Cypher) and `ox-federation` (DataFusion `LogicalPlan`).
