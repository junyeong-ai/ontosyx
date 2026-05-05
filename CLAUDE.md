# Ontosyx

Knowledge graph lifecycle platform. Rust backend (13 crates) + Next.js frontend.

Crates: `ox-core` (primitives), `ox-ontology` (OntologyIR + registries), `ox-query-ir` (QueryIR), `ox-compiler` (IR → Cypher / DataFusion lowering), `ox-runtime` (Cypher pipeline + validators), `ox-brain` (LLM routing + schema RAG), `ox-agent` (tool orchestration), `ox-memory` (temporal state), `ox-api` (HTTP gateway), `ox-store` (persistence + RLS), `ox-source` (DataSourceAdapter), `ox-federation` (DataFusion VOL path), `ox-gcp` (Application Default Credentials dispatch).

## Build & Test

```bash
cargo build --workspace          # Build all crates
cargo test --workspace           # Run all tests
cargo clippy --workspace         # Lint
cd web && pnpm install && pnpm dev  # Frontend on :3100
```

Use `./scripts/dev.sh start` to launch everything (Docker + backend + frontend).

## Key Commands

```bash
./scripts/dev.sh status          # Service dashboard
./scripts/dev.sh be restart      # Restart backend only
./scripts/dev.sh fe restart      # Restart frontend only
./scripts/dev.sh health          # API health + component status
```

## Coding Conventions

### Rust

- snake_case methods, PascalCase types.
- **Field accessors**: no `get_` prefix. `fn name(&self) -> &str`, not `fn get_name(...)`.
- **Store methods** (single source of truth — all crate `CLAUDE.md` files reference this):
  - `list_X(...)` — `Vec<X>`, cursor-paginated.
  - `get_X(id)` — single item by PK, returns `OxResult<Option<X>>`.
  - `find_X_by_Y(...)` — conditional lookup, returns `OxResult<Option<X>>`.
  - `create_X(...)` — insert, returns created row. Variants suffix the contract: `create_X_with_hash` when the caller already computed the digest, `create_X_from_template` when seeded from another row, etc.
  - `update_X(...)` — modify, returns updated row. **Never `set_*`.**
  - `upsert_X(...)` — insert-or-update on a natural key (unique constraint + `ON CONFLICT`). Only when the operation is semantically "ensure this row exists".
  - `delete_X(id)` — remove by PK.
  - **Domain verbs** (`commit_*` / `complete_*` / `archive_*` / `expire_*` / `revoke_*` / `record_*` / `aggregate_*` / `bulk_*`) are allowed when the operation has a domain meaning a CRUD verb cannot carry — `commit_version` (publishes a version), `complete_ontology_draft` (finalizes a draft), `archive_stale_proposals` (cron sweep), `bulk_revoke_active_ambiguity_resolutions` (multi-row revoke). The audit trail / fan-out / lifecycle semantics live in the verb. **Don't** invent a domain verb when the operation is a plain CRUD-equivalent — `insert_X` / `save_X` / `mark_X_*` collapse back to `create_X` / `update_X`.
- **Builders**: `with_X(...)`, `add_X(...)`, `remove_X(...)`, terminal `build() -> Result<T, _>`.
- All LLM calls go through branchforge (crates.io). Never call provider APIs directly.
- Errors propagate via `OxResult<T>`. No `unwrap()` / `expect()` / `panic!()` in library code.

### Identifier families

- **Compile-target / runtime IR**: `QueryIR`, `OntologyIR`, `PatternIR`.
- **LLM structured output**: `StructuredXxxQuery` (e.g., `StructuredMatchQuery`).
- **Input DTO** (user / LLM input pre-validation): `InputXxxDef`.
- **Analysis output**: `XxxReport`, `XxxInsight`.

### Task-local context (intentional asymmetry)

- **Store layer** uses bare names (`WORKSPACE_ID`, `SYSTEM_BYPASS`) — see `crates/ox-store/CLAUDE.md`.
- **Graph / runtime layer** uses `GRAPH_` prefix (`GRAPH_WORKSPACE_ID`, `GRAPH_SYSTEM_BYPASS`, `GRAPH_ONTOLOGY`) — see `crates/ox-runtime/CLAUDE.md`.
- Reason: a request that crosses both layers keeps postgres and graph contexts distinct in the same tokio task scope without `sync_scope` disambiguation.

### Frontend

- camelCase functions, PascalCase components, PascalCase types.
- Zustand:
  - State selectors live in `web/src/lib/store/selectors.ts` and follow `selectState<Noun>` — e.g., `selectStateOntology`.
  - Actions are accessed inline at the call-site (`useStore((s) => s.setX)` or `useStore.getState().setX()`) — the idiomatic Zustand pattern. No selector wrappers.
- Hooks: `use<Capability>` — e.g., `useGraphInteractions`.
- File-level `/* eslint-disable */` is **forbidden**. Use `// eslint-disable-next-line` only on the one line that needs it.

### Language

- **Korean** is the primary user-facing language (UI strings, error messages to end users).
- **English** for code, comments, commit messages, AI prompts, internal logs, and all documentation.

## Architecture Rules

- Dependency direction: `ox-api → ox-agent → ox-brain → ox-core`. `ox-core` has no heavy dependencies.
- `ox-brain` depends on `ox-store` (for prompt loading). `ox-store` never depends on `ox-brain`.
- Model routing: Brain uses `ModelResolver` trait. Never hardcode model names in Brain methods.
- ClientPool: keyed by provider identity (not model). Same provider shares one client.
- DB model configs (`model_configs` + `model_routing_rules`) are the source of truth for model selection at runtime.
- Workspace isolation: PostgreSQL RLS via task-local `WORKSPACE_ID`. Every workspace-scoped query respects this.
- **Workspace × Ontology cardinality is 1:1** — `UNIQUE (workspace_id)` on `ontologies`. The workspace IS the ontology context. Reach the singleton via `OntologyVersionStore::get_workspace_ontology()` (BE) / `useWorkspaceOntology()` (FE); multi-ontology-per-workspace is not a supported topology. URL surface is `/api/ontology/*` (singular, no `{id}` segment).
- **Ontology drafts track `parent_version_id`** so `complete_ontology_draft` detects intervening canonical commits (typed `ApiErrorCode::OntologyDraftStaleParent` 409). Don't write a draft-commit path that bypasses this guard — it's the lost-update lock against concurrent admin direct edits.

## Testing

```bash
docker compose up -d                                 # Required: PostgreSQL + Neo4j
cargo test --workspace                               # Unit tests
OX_TEST_DATABASE_URL=postgres://… cargo test \
    --workspace --tests -- --ignored                 # Real-Postgres integration tests
bash scripts/e2e-korean.sh                           # Korean-fixture golden lifecycle
```

The Rust test pyramid:
- Unit tests live next to the function (`#[cfg(test)] mod tests`).
- HTTP wire-shape tests use `crate::test_support::TestApp::new(Router)`
  with `mockall` per-trait fakes — narrow router, narrow state.
- Integration tests that need real Postgres are `#[ignore]` and gated
  on `OX_TEST_DATABASE_URL`. Pattern reference:
  `crates/ox-store/tests/rls_enforcement.rs`,
  `crates/ox-api/src/middleware.rs::tests`.

## Prompt Templates

TOML files in `prompts/` seed the `prompt_templates` DB table on first boot. After seeding, DB is authoritative. Edit via admin API (`/api/admin/prompts`), not by editing TOML.
