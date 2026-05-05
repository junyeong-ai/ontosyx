# ox-store

PostgreSQL persistence with Row-Level Security.

## Adding a New Store Trait

1. Define the trait in `store.rs` with async methods.
2. Add it to the `Store` supertrait (both trait def and blanket impl).
3. Implement in `postgres/<domain>.rs` — one file per trait, mirroring
   `postgres/ontology_version.rs`, `postgres/ambiguity.rs`, etc.
4. Re-export from `lib.rs`.

## Migration Conventions

- File: `migrations/NNNN_description.sql` (sequential numbering).
- Use `DOUBLE PRECISION` for monetary fields (not `NUMERIC` — sqlx maps NUMERIC to Decimal, not f64).
- Migrations auto-run on server start via `pg_store.migrate()`.

## RLS Policy Pattern (required for all workspace-scoped tables)

Every workspace-scoped table MUST have all four:
```sql
ALTER TABLE my_table ENABLE ROW LEVEL SECURITY;
ALTER TABLE my_table FORCE ROW LEVEL SECURITY;  -- even table owner obeys policies
CREATE POLICY ws_isolation ON my_table
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON my_table
    USING (current_setting('app.system_bypass', true) = 'true');
```
Missing `FORCE` silently disables RLS for the table owner role. Missing `system_bypass` blocks scheduled tasks and cross-workspace operations.

## Method Naming

See the root `CLAUDE.md` "Store methods" section — this crate is the reference implementation of that policy. Do not re-define it here.

## Task-local context

Two task-locals carry per-request state into store calls:

- `WORKSPACE_ID: Uuid` — set by the HTTP middleware (`workspace_context`).
  The pool's `before_acquire` reads it and runs `SET app.workspace_id`
  on the connection, which the RLS policies in `0004_rls.sql` read.
- `SYSTEM_BYPASS: bool` — scheduled tasks and cross-workspace admin
  paths set this to `true`. Policies whitelist `current_setting(...)
  = 'true'` so bypass reads every row.

The names intentionally have **no** `STORE_` or `PG_` prefix. The
sibling graph layer (`ox-runtime`) uses `GRAPH_WORKSPACE_ID` /
`GRAPH_SYSTEM_BYPASS` / `GRAPH_ONTOLOGY` instead, so a request that
crosses both layers keeps the postgres and graph contexts distinct in
the same tokio task scope. Reusing the same bare names across layers
would require `ox-store::WORKSPACE_ID::sync_scope(id, ws_id, ...)`
disambiguation on every single call.

## Migrations are append-only and hash-pinned

`migrations/NNNN_*.sql` files are immutable once committed.
sqlx-migrate records the SHA-256 of every applied migration in
`_sqlx_migrations.checksum`; editing a historical file fails the
checksum check on the next deploy. Schema evolution lands as a
fresh `NNNN_<focus>.sql` (N == max(existing) + 1).

The `migration_immutability` test pins every historical file's
hash through `tests/migration_baseline.json` — a git-tracked map
of `<filename>.sql → sha256(hex)`. Editing a sealed file fails the
test with the expected vs. actual hash printed for diagnosis.
Adding a new migration is a one-liner:

```
OX_UPDATE_MIGRATION_BASELINE=1 cargo test --test migration_immutability
```

The baseline regenerates from the current state of `migrations/`,
the test passes, and the resulting `migration_baseline.json` diff
gets committed alongside the new SQL file. No hand-copying hex
hashes, but the deliberate registration is still visible in the
PR diff.

Same self-bootstrapping baseline pattern as
`web/scripts/heading-primitive-audit.mjs` — one mental model for
"ratcheted invariant + JSON baseline" across the repo.

The `migrations_directory_has_no_strays` test rejects anything that
doesn't match `^\d{4}_[a-z0-9_]+\.sql$` — catches editor backups
(`*.sql.bak`) and rename leftovers before they confuse sqlx-migrate.

Don't try to "split" the historical `0001_schema.sql` monolith.
The split would mutate every historical hash and break every
existing deployment on the next migration sync. The sealed monolith
documents the v0 baseline; new domains land as fresh files.

## Workspace-scoped tables must carry full RLS protection

Every table that holds a `workspace_id` column must satisfy four
clauses (the canonical "RLS Policy Pattern" in this file):

1. `ALTER TABLE … ENABLE ROW LEVEL SECURITY`
2. `ALTER TABLE … FORCE ROW LEVEL SECURITY` — applies even to the
   table owner
3. A tenant-gate policy whose `qual` references
   `app.workspace_id` (canonical name `ws_isolation`; dual-tenancy
   tables use `ws_or_global` / `ws_write` / `ws_or_global_read` —
   the test below checks the qual SQL, not the name)
4. `system_bypass` policy for cross-workspace admin / scheduled
   tasks

`tests/rls_invariants.rs::workspace_scoped_tables_have_full_rls_protection`
is a catalog scan that fails when a new migration introduces a
`workspace_id` column without the four clauses. Runs against a
live PostgreSQL behind `OX_TEST_DATABASE_URL` (CI's `rls` job).

## Workspace × Ontology is 1:1 — singleton invariant

`ontologies(workspace_id)` carries `UNIQUE` (migration `0006`).
A workspace owns exactly one canonical ontology — the workspace
IS the ontology context. Reach the singleton via the dedicated
accessor:

```rust
let ontology = state.store.get_workspace_ontology().await?;
```

Don't add new code paths that look up by ontology id when the id
is workspace-determined. The legacy lookups (`get_ontology(id)`,
`list_ontologies`, `find_ontology_by_lineage`,
`find_ontology_by_name`) survive transitionally — Phase 3 of the
ontology workbench restructure folds them away alongside the URL
path migration that drops `/{id}/` segments.

## Project drafts pin a `parent_version_id`

`design_projects.parent_version_id` (migration `0005`) records
which canonical version a project's in-flight `ontology` JSONB
was branched from. `complete_project` compares this against the
canonical's current head and refuses commits whose parent has
been superseded — the lost-update guard against concurrent admin
direct edits via `/api/ontologies/{id}/edits`.

Capture happens at `create_project`: read
`get_workspace_ontology()` + `get_current_version()` and stamp the
result onto the new project. Greenfield workspaces (no canonical
yet) record `None`; `complete_project` then takes the
"first-version of new lineage" branch instead of the
fast-forward / refuse arms.

The typed error on stale parent is
`ApiErrorCode::ProjectStaleParent` (409) with `params.parent_version`
+ `params.current_version` so the FE renders a precise rebase
prompt.

## Pre-scope tables carry the OPPOSITE invariant

Pre-scope tables (`workspaces`, `workspace_members`, `users`) carry
the OPPOSITE invariant — RLS is forbidden on them because the auth
middleware reads them before `WORKSPACE_ID.scope` wraps the request.
The `pre_scope_tables_carry_no_rls_policies` test pins this.

## EvaluationStore — RAGAS-style metric loop

`EvaluationStore` (`store.rs`) + the `evaluation` module +
`postgres/evaluation.rs` form the platform's first-class metric
surface for LLM-driven flows (NL→Cypher translation, GraphRAG
retrieval, agent tool use). Three workspace-scoped tables, all
4-clause RLS:

- `evaluation_runs`     — one row per evaluation batch.
- `evaluation_cases`    — one row per (run, input) pair.
- `evaluation_metrics`  — one row per (case, rubric_axis) score.

The case + metric split is the RAGAS / DeepEval pattern: a case
captures the prompt-response pair plus golden expectation +
latency + error path, and 0..N metrics score it along independent
axes. Adding a new axis is a fresh INSERT, never a DDL change —
the long shape lets the evaluator record an arbitrary mix and
the operator pivot at query time.

UPSERT keys mirror the run-and-rerun cycle:

- `(run_id, case_key)` on cases — re-running a dataset replaces.
- `(case_id, name)` on metrics — re-judging replaces.

Both `parse_run_status` and `EvaluationRunStatus::is_terminal` live
on the storage enum (no utoipa dep — the API DTO accepts
`status: String` and converts via `from_wire_str`, keeping the
schema crate's dependencies minimal). The closed enum's wire
shape is snake_case; a forward deploy that tags a new variant
fails fast at parse time as `OxError::Conflict` rather than
silently downgrading to a default.

Capture hooks and the FE dashboard extend this contract without
touching the schema — the storage layer is the stable substrate
they share. The full self-service loop landed across:

1. `EvaluationContext` task-local + `EvaluationCapture` trait
   in `evaluation.rs`. Brain's `call_structured_traced` reads
   both and records `latency_ms.<operation>` whenever the
   call is inside an evaluation scope.
2. The case-execute endpoint
   (`POST /api/evaluation/runs/{run_id}/cases/{case_key}/execute`)
   binds the scope, calls the brain operation by typed `kind`
   envelope (`ExecuteEvaluationCaseRequest::TranslateQuery |
   Explain`), and lands the output / latency / error on the
   case row.
3. The judge endpoint
   (`POST /api/evaluation/cases/{case_id}/judge`) runs the
   `evaluation_judge` LLM prompt and persists each of the four
   RAGAS axes (`faithfulness`, `answer_relevance`,
   `context_precision`, `context_recall`) as an
   `evaluation_metrics` row. Re-judging UPSERTs in place
   without disturbing the latency metrics.
4. The FE dashboard at `/settings/evaluation` exposes
   create-run, case-execute (kind selector), judge (one click),
   cancel + delete actions — admin-gated end-to-end.

Adding a new operation kind extends the same axis: a new
`ExecuteEvaluationCaseRequest` variant + a brain trait + a
dispatch arm + an FE option. No schema migration, no new
endpoint per kind. Adding a new judge axis is one
`EvaluationJudgement` field + a prompt revision; the
canonical-name list in `EvaluationJudgement::axes()` is the
single source of truth the endpoint iterates.

## Advisory locks for boot-time + cron singletons

Race-prone shared-write paths use `ox_store::advisory_lock`:

- `with_advisory_lock(pool, key, fut)` — blocking. Boot-time
  seeders that must run exactly once per fresh DB
  (`ADVISORY_LOCK_PROMPT_SEED`).
- `try_advisory_lock(pool, key, fut)` — non-blocking. Cron
  singletons that should only run on one replica per tick
  (`ADVISORY_LOCK_CRON_*`). Returns `Ok(None)` when another holder
  has the lock; the caller silently skips.

Pick a fresh `ADVISORY_LOCK_*` constant when adding a new lock
surface — never inline a magic i64. The `lock_constants_are_unique`
test catches collisions.

Cron tasks override `CronTask::singleton_key()` to return
`Some(ADVISORY_LOCK_CRON_<NAME>)` when their `run_once` writes
shared state; in-process-only tasks (clarification evict,
collaboration idle reap) leave it `None` because every replica
must run on its own memory.
