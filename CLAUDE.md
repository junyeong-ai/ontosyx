# Ontosyx

Knowledge graph lifecycle platform. Rust workspace + Next.js frontend.

Crates: `ox-core` (primitives), `ox-ontology` (OntologyIR + governance surfaces), `ox-query-ir` (QueryIR + PatternIR), `ox-compiler` (IR → Cypher / DataFusion lowering), `ox-graph-runtime` (Cypher pipeline + AST validators), `ox-brain` (LLM routing + schema RAG), `ox-agent` (tool orchestration), `ox-memory` (vector / embedding), `ox-text` (Korean morphological tokenizer), `ox-source` (DataSourceAdapter), `ox-federation` (DataFusion VOL path), `ox-store` (persistence + RLS), `ox-api` (HTTP gateway).

## Build & Test

```bash
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cd web && pnpm install && pnpm dev   # frontend on :3100
```

`./scripts/dev.sh start` launches Docker + backend + frontend in one command. `./scripts/dev.sh status|health|be restart|fe restart` are the day-to-day handles.

## Coding conventions

### Rust

- snake_case methods, PascalCase types. Field accessors carry no `get_` prefix.
- **Store method vocabulary** (single source of truth — every `crates/ox-*/CLAUDE.md` defers here):
  - `list_X(...)` → `Vec<X>` cursor-paginated; `get_X(id)` → `OxResult<Option<X>>` by PK; `find_X_by_Y(...)` → conditional lookup.
  - `create_X(...)` / `update_X(...)` (never `set_*`) / `upsert_X(...)` (only when "ensure exists" is the natural-key semantic) / `delete_X(id)`.
  - **Domain verbs** (`commit_*` / `complete_*` / `archive_*` / `expire_*` / `revoke_*` / `record_*` / `aggregate_*` / `bulk_*`) are reserved for operations a CRUD verb cannot carry — audit trail / fan-out / lifecycle semantics. Don't invent a domain verb when the operation collapses back to `create_X` / `update_X`.
- **Builders**: `with_X` / `add_X` / `remove_X` then terminal `build() -> Result<T, _>`.
- All LLM calls go through branchforge (crates.io). Never call provider APIs directly.
- Library code uses `OxResult<T>`. No `unwrap()` / `expect()` / `panic!()` outside `#[cfg(test)]`.

### Identifier families (workspace-wide)

- **Compile-target / runtime IR**: `QueryIR`, `OntologyIR`, `PatternIR`.
- **LLM structured output**: `StructuredXxxQuery` (e.g. `StructuredMatchQuery`).
- **Input DTO** (user / LLM input pre-validation): `InputXxxDef`.
- **Analysis output**: `XxxReport`, `XxxInsight`.

### Task-local context (intentional asymmetry)

- **Store layer** uses bare names: `WORKSPACE_ID`, `SYSTEM_BYPASS`. See `crates/ox-store/CLAUDE.md`.
- **Graph / runtime layer** uses `GRAPH_` prefix: `GRAPH_WORKSPACE_ID`, `GRAPH_SYSTEM_BYPASS`, `GRAPH_ONTOLOGY`. See `crates/ox-graph-runtime/CLAUDE.md`.

A request crosses both layers; the prefix split lets tokio task-locals coexist without `sync_scope` disambiguation.

### Frontend

- camelCase functions, PascalCase components + types.
- Zustand: state selectors live in `web/src/lib/store/selectors.ts` and follow `selectState<Noun>`. Actions read inline at the call-site (`useStore((s) => s.setX)`) — no selector wrappers.
- Hooks: `use<Capability>` (`useGraphInteractions`).
- File-level `/* eslint-disable */` is forbidden; use `// eslint-disable-next-line` only on the offending line.

### Language

- **Korean** is the user-facing language (UI strings, errors shown to end users).
- **English** for code, comments, commit messages, AI prompts, internal logs, and all documentation.

## Architecture rules

- Dependency direction: `ox-api → ox-agent → ox-brain → ox-core`. `ox-core` carries no heavy dependencies.
- `ox-brain` depends on `ox-store` (for prompt loading). The reverse edge is forbidden.
- Brain selects models through `ModelResolver`. Never hardcode a model name.
- ClientPool keys on provider identity (not model). Same provider ↔ shared client.
- DB tables `model_configs` + `model_routing_rules` are the runtime source of truth for model selection. TOML seeds the DB on first boot only.
- Workspace isolation: PostgreSQL RLS through task-local `WORKSPACE_ID`. Every workspace-scoped query honours it.
- **Workspace × Ontology = 1:1**. `UNIQUE (workspace_id)` on `ontologies`. The workspace IS the ontology context. Reach the singleton via `OntologyVersionStore::get_workspace_ontology()` (BE) / `useWorkspaceOntology()` (FE). URL surface is `/api/ontology/*` (singular, no `{id}` segment).
- **Ontology drafts pin `parent_version_id`**. `complete_ontology_draft` rejects commits whose parent has been superseded (typed `ApiErrorCode::OntologyDraftStaleParent`, 409). The lost-update guard against concurrent admin direct edits — don't author a draft-commit path that bypasses it.

## Testing

```bash
docker compose up -d                                  # Postgres + Neo4j
cargo test --workspace                                # unit tests
OX_TEST_DATABASE_URL=postgres://… \
    cargo test --workspace --tests -- --ignored      # real-Postgres integration tests
bash scripts/e2e-korean.sh                            # Korean-fixture golden lifecycle
```

The pyramid:
- Unit tests live next to the function (`#[cfg(test)] mod tests`).
- HTTP wire-shape tests use `crate::test_support::TestApp::new(Router)` with `mockall` per-trait fakes — narrow router, narrow state. Pattern reference: `crates/ox-api/src/test_support.rs`.
- Real-Postgres integration tests are `#[ignore]` and gated on `OX_TEST_DATABASE_URL`. Pattern reference: `crates/ox-store/tests/integration/`.

## Prompt templates

TOML files in `prompts/` seed the `prompt_templates` DB table on first boot. After seeding the DB is authoritative. Edit through the admin API (`/api/admin/prompts`), not by editing TOML.

## Architecture decisions

`docs/adr/` carries the canonical record for every long-lived decision. Read `docs/adr/README.md` before changing anything load-bearing — the ADR is where the *why* lives. Adding a new decision = a new MADR-lite file `NNNN-kebab-title.md` + one row in the index, following the convention in that README.
