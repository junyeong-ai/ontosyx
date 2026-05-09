# ox-graph-runtime

Graph-DB runtime drivers (Neo4j + Memgraph via the bolt protocol).

## Pre-execute pipeline

Every `GraphRuntime::execute_query` / `execute_load` call routes through `bolt::pipeline::run_pre_execute`, which parses the incoming Cypher **once** into a `CypherAst` and threads it through three passes:

1. **Pre-rewrite validation** (`run_pre_rewrite_validation`) — `SafetyValidator` (always) + `OntologyValidator` (when `GRAPH_ONTOLOGY` is bound).
2. **Rewrite** (`GraphIsolationStrategy::scope_ast`) — `PropertyStrategy` injects workspace predicates (`WHERE var._workspace_id = $_ws_id`) and assignments (`SET var._workspace_id = $_ws_id`). `DatabaseStrategy` is passthrough. Returns `Result<ScopedAst, RewriteError>` so a future strategy (ACL, soft-delete, temporal) can refuse a query it can't scope safely.
3. **Post-rewrite scope check** — uses the structural `ScopedAst.modified_statements` count. When a strategy declares a `scope_property` and any statement touches graph data, the rewriter's count must be non-zero. A literal `RETURN "_workspace_id"` cannot satisfy the gate by accident.

Only after all three passes does the pipeline call `ast.render()`. One parse, three passes — the design keeps rewriter + validator behaviour composable without paying the parse cost three times. Errors from any pass collapse into a single `OxError::Validation { field: "cypher_query", message }` with every issue on its own line, so one LLM retry can fix all of them.

The pipeline **refuses execution** when a strategy declares a `scope_property` (isolation must show up in the rewritten text) but no `GRAPH_WORKSPACE_ID` is bound and we're not in system-bypass — returns `OxError::Runtime` naming the missing task-local rather than silently passing the query through.

## Phase-ordered rewriter / validator pipelines

Both `CypherRewriterPipeline` and `CypherValidatorPipeline` stable-sort their passes by an explicit phase slot before execution:

- `RewritePhase`: `Isolation(100)` → `Acl(200)` → `SoftDelete(300)` → `Temporal(400)` → `Custom(900)`.
- `ValidatePhase`: `PreRewriteSafety(100)` → `PreRewriteOntology(200)` → `PostRewrite(900)`.

Adding a new pass = a new file + one `impl` + one `.with()` call. The trait's `phase()` pins the slot so wiring can live anywhere.

## Strategy parameter merge — strategy-wins

`ScopedAst.params: Vec<(String, String)>` lets a strategy introduce bind parameters during rewriting. `run_pre_execute` merges these into the caller's `params: HashMap<String, PropertyValue>` with **strategy-wins semantics** — a user-supplied `_ws_id` cannot spoof the rewriter's value. Collisions log at `warn` level with the strategy name and parameter key; the query still runs safely with the strategy value.

## Task-locals

Three task-locals drive the pipeline:

- `GRAPH_WORKSPACE_ID: Uuid` — per-request via `ContextScope` / middleware.
- `GRAPH_SYSTEM_BYPASS: bool` — skips isolation for system tasks (migrations, health checks).
- `GRAPH_ONTOLOGY: Arc<OntologyIR>` — active ontology snapshot. Unset = ontology validator skipped (server-internal paths like `search_nodes`, profiler, introspection rely on this). The agent loads it via `DomainContext::current_ontology()` at the start of each tool call so mid-session edits propagate on the next invocation.

The `GRAPH_` prefix mirrors `ox-store`'s bare `WORKSPACE_ID` / `SYSTEM_BYPASS` so a request crossing both layers can capture both pairs without disambiguating the same identifier twice.

## OntologyValidator covers every read/write surface

Cypher's schema-less driver writes unknown properties on `SET` and reads zero rows on a typo'd `WHERE` — both silent. `OntologyValidator` rejects pre-execute with a structured diagnostic. Coverage:

- **Patterns** — node label / inline property (`(p:Person {emial: 'x'})`), relationship type / inline property (`[r:WORKS_AT {sicne: 2020}]`).
- **Writes** — `SET` / `REMOVE` on node *and* relationship variables. `MERGE`'s `ON CREATE SET` / `ON MATCH SET` slip out as free-standing Set clauses, so the same SET walk catches them.
- **Reads** — `WHERE` / `RETURN` / `WITH` / `ORDER BY` token-level `<id>.<id>` triples. Conservative on purpose: locally-bound variables (list comprehension, EXISTS subquery, WITH-introduced) silently skip — false-positive avoidance over false-negative coverage.
- **Noise suppression** — when every label / type on an element is unknown, the property walk is skipped; the `unknown_label` / `unknown_type` diagnostic already names the real fix-site.

Variable resolution is shared. `CypherStatement::variable_labels()` / `variable_relationship_types()` walk the pattern-bearing clauses once per parse and both the SHACL validator and the ontology validator consume the same maps.

## Diagnostic shape — `subject_kind` + `subject_name`

Every property-typo diagnostic emits two stable params:

- `subject_kind` ∈ `"node"` | `"relationship"` — the FE i18n catalogue dispatches on this via ICU `select` so `subject_name` reads as `라벨 'Foo'` or `관계 타입 'BAR'` in Korean without the BE having to interpolate either fragment.
- `subject_name` — the offending label list (`"Person"` / `"Person/Customer"`) or type list (`"WORKS_AT"` / `"KNOWS|WORKS_AT"`).

`unknown_read_property` carries an extra `clause` param (`"WHERE"` / `"RETURN"` / `"WITH"` / `"ORDER BY"`). Don't introduce a fresh `unknown_..._on_node` / `unknown_..._on_relationship` pair — the existing ICU select keeps both arms in one entry.

## SHACL constraint enforcement

`ShaclValidator` enforces every `ShaclConstraint` variant whose decision can be made at AST time on the `SET` / inline-pattern / `CREATE` / `MERGE` write surfaces. Adding a new kind:

1. Add the variant to `ShaclConstraint` in `ox_ontology::rule`.
2. Decide the dedup signature: opt-in via `ConstraintSignature` or stay `None` (the dedup pipeline never collapses two `None`-signed constraints).
3. Pull the SET / inline value through one of the three classifiers in `cypher::shacl_validator` — `parse_string_literal`, `parse_numeric_literal`, `classify_literal_type`.
4. Add the arm to `check_property_constraint`. **Silent skip on non-decidable values** (parameter, function call, null, identifier reference) — false-positive avoidance trumps false-negative coverage.
5. Emit a typed diagnostic `runtime.cypher.shacl.<kind>_violation` with stable params + an i18n entry in `web/messages/{en,ko}.json` (`pnpm i18n-parity-audit` gates lockstep).

`MaxCount` is deliberately not enforced at AST layer — it needs the graph-instance count, which the AST cannot see. A future runtime-side companion check belongs in the bolt driver.

## Adding a graph backend

1. Implement `GraphRuntime` (schema DDL, query execution, load, sandbox, health).
2. Implement `TransienceDetector` for error classification (transient vs permanent).
3. Register in `registry.rs`.

## Enrichment

`enrichment.rs` post-processes query results: resolves node labels, adds display names, formats temporal values. Applied after execution, before returning to the agent.
