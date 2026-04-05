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

- Rust: snake_case methods, PascalCase types. No `get_` prefix on getters.
- Store trait verbs: `list_*`, `get_*`, `find_*`, `create_*`, `update_*`, `delete_*`.
- Frontend: camelCase functions, PascalCase components. Zustand for state.
- All LLM calls go through branchforge (crates.io). Never call LLM APIs directly.
- Errors propagate via `OxResult<T>`. No `unwrap()` or `expect()` in library code.
- Korean is the primary user language. English for code, comments, and docs.

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
