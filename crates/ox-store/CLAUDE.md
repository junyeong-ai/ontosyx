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
