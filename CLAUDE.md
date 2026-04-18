# Ontosyx

Knowledge graph lifecycle platform. Rust backend (9 crates) + Next.js frontend.

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
  - `create_X(...)` — insert, returns created row.
  - `update_X(...)` — modify, returns updated row. **Never `set_*`.**
  - `upsert_X(...)` — insert-or-update on a natural key (unique constraint + `ON CONFLICT`). Only when the operation is semantically "ensure this row exists".
  - `delete_X(id)` — remove by PK.
- **Builders**: `with_X(...)`, `add_X(...)`, `remove_X(...)`, terminal `build() -> Result<T, _>`.
- All LLM calls go through branchforge (crates.io). Never call provider APIs directly.
- Errors propagate via `OxResult<T>`. No `unwrap()` / `expect()` / `panic!()` in library code — workspace lints are gated to `deny` once Phase 1 clean-up completes.

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
- Zustand selectors:
  - State selectors: `selectState<Noun>` — e.g., `selectStateOntology`.
  - Action selectors: `selectAction<Verb>` — e.g., `selectActionSetOntology`.
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

## Testing

```bash
docker compose up -d                        # Required: PostgreSQL + Neo4j
cargo test --workspace                      # Unit tests
./scripts/e2e-test.sh                       # API integration tests
./scripts/e2e-full.sh                       # Full lifecycle test
```

## Prompt Templates

TOML files in `prompts/` seed the `prompt_templates` DB table on first boot. After seeding, DB is authoritative. Edit via admin API (`/api/admin/prompts`), not by editing TOML.
