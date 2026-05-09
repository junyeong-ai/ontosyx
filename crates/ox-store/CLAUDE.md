# ox-store

PostgreSQL persistence with Row-Level Security.

## Adding a new store trait

1. Define the trait in `store/<domain>.rs` with async methods.
2. Add it to the `Store` supertrait in `store/mod.rs` (both bound and blanket impl).
3. Implement in `postgres/<domain>.rs` — one file per trait.
4. Re-export from `lib.rs`.

Method-naming vocabulary (`list_X` / `get_X` / `find_X_by_Y` / `create_X` / `update_X` / `upsert_X` / `delete_X` plus reserved domain verbs) lives in the root `CLAUDE.md` — this crate is its reference implementation. Don't re-define the policy here.

## Schema baseline

`migrations/0001_schema.sql` is the canonical development schema. It auto-runs on server start via `pg_store.migrate()`. Use `DOUBLE PRECISION` for monetary columns — sqlx maps `NUMERIC` to `Decimal`, not `f64`.

Migrations are append-only and pinned by SHA-256 in `tests/migration_immutability.rs`. Schema changes ship as a new file, never an edit to a historical one.

## RLS policy pattern (required for every workspace-scoped table)

```sql
ALTER TABLE my_table ENABLE ROW LEVEL SECURITY;
ALTER TABLE my_table FORCE ROW LEVEL SECURITY;       -- even table owner obeys
CREATE POLICY ws_isolation ON my_table
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON my_table
    USING (current_setting('app.system_bypass', true) = 'true');
```

Missing `FORCE` silently disables RLS for the table owner role. Missing `system_bypass` blocks scheduled tasks and cross-workspace admin paths. Dual-tenancy tables substitute `ws_or_global` / `ws_write` / `ws_or_global_read` for the gate policy — the catalog test checks the SQL, not the policy name.

`tests/integration/rls_invariants.rs::workspace_scoped_tables_have_full_rls_protection` is a live-Postgres catalog scan that fails when a `workspace_id` column ships without all four clauses.

**Pre-scope tables** (`workspaces`, `workspace_members`, `users`) carry the OPPOSITE invariant — RLS is forbidden because the auth middleware reads them before `WORKSPACE_ID.scope` wraps the request. Pinned by `pre_scope_tables_carry_no_rls_policies`.

## Task-local context

Two task-locals carry per-request state:

- `WORKSPACE_ID: Uuid` — set by the HTTP middleware (`workspace_context`). The pool's `before_acquire` hook reads it and runs `SET app.workspace_id` on the connection.
- `SYSTEM_BYPASS: bool` — scheduled tasks and cross-workspace admin paths set this to `true`; policies whitelist `current_setting(...) = 'true'`.

Names are bare on purpose. The graph layer (`ox-graph-runtime`) uses `GRAPH_WORKSPACE_ID` / `GRAPH_SYSTEM_BYPASS` / `GRAPH_ONTOLOGY` so a request crossing both layers keeps the postgres and graph contexts distinct in the same tokio scope.

## Workspace × Ontology = 1:1

`ontologies(workspace_id)` carries `UNIQUE`. Resolve through the singleton accessor:

```rust
let ontology = state.store.get_workspace_ontology().await?;
```

Don't add new lookup paths keyed by ontology id when the id is workspace-determined.

## Ontology drafts pin `parent_version_id`

`ontology_drafts.parent_version_id` records which canonical version a draft branched from. `complete_ontology_draft` compares it against the current head and refuses commits whose parent has been superseded — the lost-update guard against concurrent admin direct edits via `/api/ontology/edits`.

`create_ontology_draft` stamps it from `get_workspace_ontology() + find_current_version()`. Greenfield workspaces (no canonical yet) record `None` and `complete_ontology_draft` takes the "first-version of new lineage" branch.

The typed error on stale parent is `ApiErrorCode::OntologyDraftStaleParent` (409) with `params.parent_version` + `params.current_version` so the FE renders a precise rebase prompt.

## EvaluationStore — RAGAS-style metric loop

Three workspace-scoped tables, all 4-clause RLS:

- `evaluation_runs` — one row per evaluation batch.
- `evaluation_cases` — one row per `(run, input)` pair. UPSERT key `(run_id, case_key)` so a re-run replaces in place.
- `evaluation_metrics` — one row per `(case, rubric_axis)` score. UPSERT key `(case_id, name)` so re-judging replaces without disturbing latency rows.

Adding a new judge axis = one `EvaluationJudgement` field + one prompt-template revision. `EvaluationJudgement::axes()` is the canonical iterator the case-judge endpoint walks. Adding a new operation kind = one `ExecuteEvaluationCaseRequest` variant + a brain trait method + a dispatch arm. No schema migration per axis or kind.

ADR-0018 carries the *why* and full lifecycle (capture hook → case-execute → judge → dashboard).

## Advisory locks for boot-time + cron singletons

`ox_store::advisory_lock`:

- `with_advisory_lock(pool, key, fut)` — blocking. Boot-time seeders that must run exactly once per fresh DB (`ADVISORY_LOCK_PROMPT_SEED`).
- `try_advisory_lock(pool, key, fut)` — non-blocking. Cron singletons (`ADVISORY_LOCK_CRON_*`); returns `Ok(None)` when another holder has the lock and the caller silently skips.

Pick a fresh `ADVISORY_LOCK_*` constant — never inline a magic `i64`. `lock_constants_are_unique` catches collisions. Cron tasks override `CronTask::singleton_key()` to return `Some(ADVISORY_LOCK_CRON_<NAME>)` when their `run_once` writes shared state.

## Closed-set wire enums — `wire_enum!` macro

Every closed-set enum that crosses HTTP / SQL / log goes through `crate::wire_enum!`. The macro emits `ALL` + `as_str(self) const fn` + `from_wire_str` + `all_wire_strings` + `Display` + serde + utoipa derives, with explicit per-variant wire literals on both `#[serde(rename)]` and `#[schema(rename)]`:

```rust
crate::wire_enum! {
    pub enum MyKind {
        First => "first",
        Second => "second_variant",
    }
}
```

The wire literal at the declaration site is the single source of truth — serde, utoipa OpenAPI, and `as_str` stay in lock-step. Per-enum extras (`is_terminal`, `from_subscription`, …) ride in a separate `impl` block beside the macro invocation. The macro's contract is pinned by `wire_enum::tests` against a probe enum so a future macro change that drops a method or breaks the round-trip fails before any downstream enum compiles.
