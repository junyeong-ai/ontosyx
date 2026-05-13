# ox-core

Primitives and shared infrastructure. Zero heavy dependencies — every other crate depends on this, and this crate depends on nothing else in the workspace.

This crate holds only the "what every crate needs" foundation.
Domain modelling (ontology, query algebra, patterns, mapping,
governance) lives in sibling crates:

- `ox-ontology` — `OntologyIR`, node/edge/property defs, rules,
  mappings, provenance, glossary, data quality, etc.
- `ox-query-ir` — `QueryIR`, `PatternIR`, bindings, structured
  match form used by LLMs.

The layering arrow `ox-core ← ox-ontology ← ox-query-ir` is enforced
by `deny.toml::bans.deny`; do not introduce a reverse edge.

## What lives here

- `error` — `OxError` (with `with_context`), `OxResult<T>`.
- `graph_label` — validated `GraphLabel` identifier newtype.
- `property_key` — validated `PropertyKey` newtype.
- `variable_name` — validated `VariableName` newtype.
- `id` — the `define_id_newtype!` macro downstream crates use to
  derive typed ids (`NodeTypeId`, `PropertyId`, etc.) with
  consistent `Deref`, `PartialEq<str>`, and `Display` impls.
- `i18n` — `LanguageTag`, `LocaleError`, `LocalizedText` — Rust
  representation of localised-string columns.
- `prompt_version` — `PromptVersion` semver wrapper (used by
  `prompt_templates`).
- `source_schema` — `SourceSchema`, `SourceProfile`, `ColumnStats` —
  the minimal schema-snapshot types `ox-source` introspection
  produces and `ox-ontology::value_set_inference` etc. consume.
- `types` — cypher identifier sanitisation helpers.

## Error Handling

All errors use `OxResult<T>` = `Result<T, OxError>`. The variants
are closed and grouped by surface:

- **Compilation / Runtime / Validation / Parse / Serialization /
  UnsupportedOperation** — the standard domain failure modes;
  `Validation` carries `(field, message)`, `Parse` the source-error
  chain.
- **Ontology / NotFound / Conflict** — typed predicates the API
  boundary maps onto `4xx` codes (`ontology_error`, `not_found`,
  `conflict`).
- **MissingContext** — a workspace / system-bypass scope was
  required but unset. Surfaces as `5xx missing_context` because the
  caller path forgot to wrap in a scope.
- **Llm** — typed LLM-side failure carrying [`LlmErrorCode`],
  operator detail, and an optional `retry_after_secs`. The API
  boundary maps `code` onto an `ApiErrorCode::Llm*` variant and the
  FE i18n catalogue renders the prose; operator detail never
  reaches the wire body.
- **Contextual** — `{ source: Box<OxError>, target, location }`
  wrapper used at layer boundaries. Nested wraps flatten so only
  the outermost target/location survives.

No `unwrap()` or `expect()` in this crate.

### `with_context` at layer boundaries

When an `OxError` crosses a layer boundary (adapter → kernel,
kernel → compile, compile → runtime, runtime → api, brain → api),
add a single `.with_context(target, location)` call on the way out.
The pattern:

```rust
adapter
    .primitive()
    .await
    .map_err(|e| e.with_context("source:postgres", "introspection_kernel::analyze"))?
```

Rules:
- **One context wrap per boundary**, not per line. Nested wraps
  flatten — only the outermost target/location is kept — so extra
  calls are harmless but noisy.
- **target** is `<layer>:<backend>` (`source:postgres`,
  `graph:neo4j`, `brain:anthropic`) — the "which dependency failed"
  axis.
- **location** is `<module>::<fn>` — the "where in our code did the
  boundary live" axis.
- Do not wrap every `?` inside a function. Context is load-bearing
  at *layer* boundaries, not statement boundaries;
  `tracing::instrument` fields carry the intra-function detail.

Reference implementation: `crates/ox-source/src/kernel.rs::run_analyze_with_retry`.

## Don't

- Don't add new domain types here. If the type names a kind of
  node, edge, property, rule, query shape, or report, it belongs in
  `ox-ontology` or `ox-query-ir`.
- Don't depend on any other workspace crate. `ox-core` is the
  bottom of the dependency graph.
