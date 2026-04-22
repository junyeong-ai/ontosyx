# ADR 0005: Final naming convention

- Status: Accepted
- Date: 2026-04-20

## Context

Across v1/v2 the Ontosyx codebase accumulated coexisting naming schemes:

- `OntologyDef` vs `OntologyIR` vs `InputOntologyDef` with overlapping
  roles.
- `BusinessRule` as an ad-hoc enum sitting next to a struct-shaped
  `NodeConstraint`.
- `RuleSeverity`, `RuleEnforcement`, `SourceRelation`, `LinkKind` — some
  with `-Kind` suffix, some without.
- Frontend Zustand selectors named `selectX` despite CLAUDE.md
  specifying `selectStateX` / `selectActionX`.
- Rust `fn name()` field accessors coexisting with an older generation
  of `get_` prefixed accessors (now removed, but the habit returns in
  new code).

Naming debt compounds. Every new identifier that violates a convention
gives the next author permission to violate it. This ADR pins the
convention so the rule is not "what did we do before" but "what does
0005 say."

## Decision

The following table is normative. PRs that introduce a new identifier
violating it are rejected at review; a codemod (Phase 1) brings the
current tree into compliance.

### Types

| Role                              | Pattern                     | Examples |
|-----------------------------------|-----------------------------|----------|
| Stable identifier                 | `XxxId`                     | `NodeTypeId`, `ObjectMappingId`, `RuleId` |
| Definition / metadata             | `XxxDef`                    | `NodeTypeDef`, `RuleDef`, `ActionDef` |
| User / LLM input DTO              | `InputXxxDef`               | `InputNodeTypeDef`, `InputRuleDef` |
| Runtime IR (compile target)       | `XxxIR`                     | `OntologyIR`, `QueryIR`, `PatternIR`, `PlanIR` |
| LLM structured output             | `StructuredXxx<Artifact>`   | `StructuredMatchQuery`, `StructuredOntologyDraft` |
| Analysis / report output          | `XxxReport`, `XxxInsight`   | `SchemaDriftReport`, `QueryCostReport`, `DataQualityReport` |
| Enum tag                          | `XxxKind`                   | `RuleKind`, `ActionKind`, `LinkMappingKind`, `PropertyTransformKind`, `PartialFailureKind` |
| Qualitative scale / grade enum    | bare noun                   | `SemanticType`, `Severity`, `Classification`, `Cardinality` |
| Path / reference value            | `XxxRef`                    | `ColumnRef`, `TableRef`, `EndpointRef`, `AgentRef` |

### Behavioural objects

| Role                    | Suffix        | Examples |
|-------------------------|---------------|----------|
| Orchestrates a pipeline | `Planner`     | `QueryPlanner`, `ExecutionPlanner` |
| Resolves ids to content | `Resolver`    | `MappingResolver`, `OntologyResolver` |
| Builds an IR / plan     | `Builder`     | `LogicalPlanBuilder`, `PatternBuilder` |
| Splits a hard problem   | `Decomposer`  | `PathDecomposer` |
| Emits / injects clauses | `Injector`    | `WorkspacePredicateInjector` |
| Rewrites IR / AST       | `Rewriter`    | `TemporalRewriter`, `RenameRewriter` |
| Checks invariants       | `Validator`   | `RuleValidator`, `MappingValidator` |
| Estimates cost          | `Estimator`   | `CostEstimator` |
| Executes an action      | `Executor`    | `ActionExecutor`, `PlanExecutor` |
| Classifies input        | `Typer`       | `SemanticTyper` |
| Consumes a plan batch   | `Dispatcher`  | `BackendDispatcher` |
| Streams provenance      | `Tagger`      | `ProvenanceTagger` |
| Shapes result to model  | `Shaper`      | `ResultShaper` |
| Suggests via LLM        | `Enricher`    | `GlossaryEnricher` |
| Long-running job        | `Reconciler`  | `SchemaDriftReconciler` |
| Cache / store materials | `Materializer`| `GraphCacheMaterializer` |

### Extension points (traits)

`<Concept><Role>` where `Role ∈ {Adapter, Provider, Backend, Source, Sink, Engine, Service, Store, Repository}`.

| Role      | Use case |
|-----------|----------|
| `Adapter` | Translates across an external wire (`DataSourceAdapter`) |
| `Provider`| Fulfils an input contract (`TableProvider`, `EmbeddingProvider`) |
| `Backend` | Swappable execution target (`GraphCacheBackend`) |
| `Engine`  | Domain-logic core (`RuleEngine`, `InferenceEngine`) |
| `Service` | Cross-cutting capability (`ApprovalService`) |
| `Store`   | Persistence with `list_/get_/find_/create_/update_/upsert_/delete_` shape |
| `Repository` | Domain-oriented read model on top of a store |

### Store method family (from CLAUDE.md, reaffirmed)

`list_X(...)`, `get_X(id)`, `find_X_by_Y(...)`, `create_X(...)`,
`update_X(...)`, `upsert_X(...)`, `delete_X(id)`.

Never `set_X`. Never `save_X` (it is ambiguous between create / update /
upsert). Never `get_` prefix on a field accessor.

### Builders

`with_X(...)`, `add_X(...)`, `remove_X(...)`, terminal `build() -> Result`.

### Task-local scopes

| Layer            | Prefix   | Rationale |
|------------------|----------|-----------|
| `ox-store`       | bare     | `WORKSPACE_ID`, `SYSTEM_BYPASS` — the task's DB identity |
| `ox-federation`  | `FED_`   | `FED_WORKSPACE_ID`, `FED_ONTOLOGY` — the task's logical view |
| `ox-runtime`     | `GRAPH_` | `GRAPH_WORKSPACE_ID` — the task's graph-cache identity |

A single request may enter all three layers; the prefixes keep the
contexts distinguishable without `sync_scope` disambiguation.

### Audit events

Past-tense verb phrase: `OntologyCreated`, `MappingUpdated`,
`ActionExecuted`, `RuleViolated`.

### Frontend

| Role                | Pattern                         |
|---------------------|---------------------------------|
| Zustand selector    | `selectState<X>`, `selectAction<X>` |
| Hook                | `use<Capability>` |
| Component           | PascalCase noun phrase (`MappingStudio`, `RuleStudio`) |
| i18n key            | `<namespace>.<subject>.<actionOrState>` |
| Prompt file         | snake_case verb phrase `.toml` (`enrich_glossary.toml`, `propose_shapes.toml`) |

### Removed / renamed (no back-compat shims)

- `BusinessRule` enum — **deleted** (superseded by SHACL-aligned
  `RuleDef { kind: RuleKind, body: RuleBody }`; see ADR 0006).
- `RuleSeverity` → `Severity`.
- `RuleEnforcement` → `EnforcementKind`.
- `ObjectMapping` (as originally sketched) → `ObjectMappingDef`.
- Zustand `selectX` → `selectStateX` / `selectActionX` via Phase 1
  codemod.

## Consequences

### Positive

- Every new identifier has an obvious shape. Reviewers and tools can
  check mechanically.
- Cross-crate refactors stop stalling on "what should this be called."
- The Rust↔TypeScript boundary reads symmetrically (`XxxDef` → DTO in
  both languages).

### Negative

- One-time codemod cost. Phase 1 owns it.
- A few historical names (`OntologyIR` vs `OntologyDef`) survive
  because they already follow the rule; readers have to accept the
  distinction once and then it stays true.

## Related

- CLAUDE.md — project conventions, this ADR is the definitive source.
- ADR 0006 — SHACL rule model (removes `BusinessRule` enum).
