# ox-runtime

Graph DB runtime drivers. Currently: Neo4j + Memgraph (both via bolt protocol).

## Pre-Execute Pipeline

Every `GraphRuntime::execute_query` / `execute_load` call routes through `bolt::pipeline::run_pre_execute`, which parses the incoming Cypher **once** into a `CypherAst` and threads that AST through three passes:

1. **Pre-rewrite validation** (`run_pre_rewrite_validation`): `SafetyValidator` (always) + `OntologyValidator` (iff `GRAPH_ONTOLOGY` task-local is set). Runs on the freshly-parsed AST.
2. **Rewrite** (`GraphIsolationStrategy::scope_ast`): the active strategy injects workspace predicates / property assignments on the AST directly. `PropertyStrategy` adds `WHERE var._workspace_id = $_ws_id` / `SET var._workspace_id = $_ws_id`; `DatabaseStrategy` is a passthrough. Returns `Result<ScopedAst, RewriteError>` so a future strategy (ACL, soft-delete, temporal) can refuse a query it can't scope safely.
3. **Post-rewrite scope check**: uses the structural `ScopedAst.modified_statements` count. When a strategy declares a `scope_property` and any statement touches graph data, the rewriter's count must be non-zero. The old substring-based `WorkspaceScopeValidator` is gone — a literal `RETURN "_workspace_id"` no longer satisfies the gate by accident.

Only after all three passes does the pipeline call `ast.render()` to produce the final Cypher text. One parse, three passes — the triple-surface design documented in `cypher/` keeps rewriter and validator behaviour composable without paying the parse cost three times.

Errors from any pass collapse into a single `OxError::Validation { field: "cypher_query", message }` with every issue on its own line — one LLM retry can fix all of them.

The pipeline **refuses execution** when a strategy declares a `scope_property` (i.e. isolation must show up in the rewritten text) but no `GRAPH_WORKSPACE_ID` is bound and we're not in system-bypass. Returns `OxError::Runtime` naming the missing task-local rather than silently passing the query through.

## Phase-Ordered Rewriter / Validator Pipelines

Both `CypherRewriterPipeline` and `CypherValidatorPipeline` stable-sort their passes by an explicit phase slot before execution, so registration order only breaks ties within the same phase:

- `RewritePhase` — `Isolation(100)` → `Acl(200)` → `SoftDelete(300)` → `Temporal(400)` → `Custom(900)`.
- `ValidatePhase` — `PreRewriteSafety(100)` → `PreRewriteOntology(200)` → `PostRewrite(900)`.

Adding a new pass is "new file + one `impl` + one `.with()` call" — the trait's `phase()` method pins the slot so wiring can live anywhere without coupling to invisible file ordering.

## Strategy Parameter Merge Policy

`ScopedAst.params: Vec<(String, String)>` lets a strategy introduce any number of bind parameters during rewriting. `run_pre_execute` merges these into the caller's original `params: HashMap<String, PropertyValue>` with **strategy-wins semantics** — if a user-supplied parameter collides with a rewriter-injected one (e.g. both name `_ws_id`), the rewriter's value overwrites. This is intentional: scope parameters are system-critical and must not be spoofed from the outside. Collisions are logged at `warn` level with the strategy name and parameter key so operators can find the offending caller; the query still runs safely with the strategy value.

## Task-Locals

Three task-locals drive the pipeline:
- `GRAPH_WORKSPACE_ID: Uuid` — injected per-request via branchforge's `ContextScope` / middleware. Drives the rewriter.
- `GRAPH_SYSTEM_BYPASS: bool` — skips isolation for system tasks (migrations, health checks).
- `GRAPH_ONTOLOGY: Arc<OntologyIR>` — active ontology snapshot. When unset, the ontology validator is skipped (server-internal paths like `search_nodes`, profiler, introspection rely on this). The agent tool loads it once per tool call via `DomainContext::current_ontology()` so mid-session edits (apply_ontology / edit_ontology) propagate on the next invocation.

The `GRAPH_` prefix is intentional. `ox-store` also owns
`WORKSPACE_ID` / `SYSTEM_BYPASS` task-locals for its Postgres RLS
layer. A request typically crosses both layers in the same tokio task
scope; keeping the graph-layer names prefixed ensures spawn helpers
(`spawn_scoped::spawn_with_ws`) can capture both pairs without
disambiguating the same bare identifier twice.

## Enrichment

`enrichment.rs` post-processes query results: resolves node labels, adds display names, formats temporal values. Applied after execution, before returning to the agent.

## OntologyValidator covers every read/write surface

`OntologyValidator` is the platform's defence against
LLM-hallucinated identifiers. It walks every surface where a
typo would silently land — Cypher's schema-less driver writes
unknown properties on SET / leaves WHERE on a typo as zero rows
— and rejects the query pre-execute with a structured diagnostic.

Coverage:

- **Patterns** — node label / inline property
  (`(p:Person {emial: 'x'})`), relationship type / inline property
  (`[r:WORKS_AT {sicne: 2020}]`).
- **Writes** — SET / REMOVE on node *and* relationship variables
  (`SET u.emial = 'x'`, `SET r.sicne = 2020`). MERGE's
  `ON CREATE SET` / `ON MATCH SET` slip out as free-standing
  Set clauses (the parser doesn't fold ON into MERGE), so the
  same SET walk catches them — pinned by regression tests.
- **Reads** — WHERE / RETURN / WITH / ORDER BY token-level
  `<id>.<id>` triples. Conservative on purpose: locally-bound
  variables (list comprehension `[x IN coll | x.y]`, EXISTS
  subquery, WITH-introduced) silently skip — false-positive
  avoidance over false-negative coverage.
- **Noise suppression** — when every label / type on an element
  is unknown the property walk is skipped; the unknown_label /
  unknown_type diagnostic already names the real fix-site, and
  a follow-up "property X not defined on Userr" only crowds the
  LLM-retry prompt.

Variable resolution is shared. `CypherStatement::variable_labels()`
(node) + `variable_relationship_types()` (edge) walk the
pattern-bearing clauses once per parse, and both the SHACL
validator and the ontology validator consume the same maps —
adding a new check that needs to know "what type is `r` bound
to?" never re-implements the walk.

## Diagnostic shape — `subject_kind` + `subject_name`

Every property-typo diagnostic emits two stable params:

- `subject_kind` ∈ {`"node"`, `"relationship"`} — the surface
  the typo lives on. The FE i18n catalog dispatches on this
  via ICU `select` so `subject_name` reads as either `라벨
  ‘Foo’` or `관계 타입 ‘BAR’` in Korean without the BE having
  to interpolate either fragment.
- `subject_name` — the offending label list (`"Person"` /
  `"Person/Customer"`) or type list (`"WORKS_AT"` /
  `"KNOWS|WORKS_AT"`).

`unknown_read_property` carries an extra `clause` param
(`"WHERE"` / `"RETURN"` / `"WITH"` / `"ORDER BY"`) so the
catalog can name the surface. Don't introduce a fresh
`unknown_..._on_node` / `unknown_..._on_relationship` pair when
the existing code already exists — the ICU select keeps both
arms in one entry, and the `feedback_set_remove_ontology_check`
memory documents the contract.

## SHACL constraint enforcement matrix

`ShaclValidator` enforces every constraint kind that can be
decided at AST time on the SET / inline-pattern / CREATE / MERGE
write surfaces. Adding a new constraint kind that fits the same
shape (compares an authored expectation against a literal value)
follows a fixed playbook:

1. Add the variant to `ShaclConstraint` in `ox_ontology::rule`.
2. Decide the dedup signature: opt-in via `ConstraintSignature`
   or stay `None` (the dedup pipeline never collapses two
   `None`-signed constraints — opt-in is the durable contract).
3. Pull the SET / inline value through one of the three
   classifiers in `cypher::shacl_validator`:
   - `parse_string_literal(raw)` — `'foo'` / `"foo"` → unquoted
     string. Returns `None` for non-quoted tokens.
   - `parse_numeric_literal(raw)` — `42` / `3.14` → `f64`.
     Returns `None` for strings, params, function calls, null,
     bool literals.
   - `classify_literal_type(raw)` — `Option<PropertyType>` for
     all 10 wire variants. Used by `Datatype` and the
     `InValueSet` / `HasValue` numeric+bool fallback.
4. Add the arm to `check_property_constraint`. **Silent skip on
   non-decidable values** (parameter, function call, null,
   identifier reference) — the matching `Datatype` rule (if
   any) catches the type-axis violation independently. False-
   positive avoidance trumps false-negative coverage.
5. Emit a typed diagnostic with `runtime.cypher.shacl.<kind>_violation`
   and stable params (the rule's id + name + the rule-specific
   bound + the observed `value`). Add the catalog entry to
   `web/messages/{en,ko}.json` — `pnpm i18n-parity-audit`
   gates the lockstep.

Currently enforced (one assertion site, one diagnostic):

- `MinCount` (also derived implicitly from `nullable=false` —
  see `ox_ontology::derived_rules::derive_nullable_rules`)
- `Datatype` — literal type widening matrix in `type_assignable`
- `InValueSet` — string + numeric + bool literal membership
- `MatchesPattern` — notation-pattern regex
- `LessThan` / `Equals` — sibling-property predicate emit
- `Closed` — node-shape allow-list
- `Disjoint` / `UniqueKey` / `UniqueLang` — node-level
- `MinInclusive` / `MaxInclusive` — numeric bounds
- `MinLength` / `MaxLength` — codepoint-counted string length
- `HasValue` — single authored equality

Deliberately not enforced at AST layer:

- `MaxCount` — needs the graph-instance count, which the
  AST cannot see; the validator only inspects one write at a
  time and the count would race with concurrent writers.
  A future runtime-side companion check belongs in the bolt
  driver, not the AST validator.

## Adding a New Graph Backend

1. Implement `GraphRuntime` trait (schema DDL, query execution, load, sandbox, health).
2. Implement `TransienceDetector` for error classification (transient vs permanent).
3. Register in `registry.rs`.
