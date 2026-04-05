# ox-store

PostgreSQL persistence with Row-Level Security.

## Adding a New Store Trait

1. Define the trait in `store.rs` with async methods.
2. Add it to the `Store` supertrait (both trait def and blanket impl).
3. Implement in `postgres.rs`.
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

- `list_*` — return Vec, cursor-paginated.
- `get_*` — return single item by ID.
- `find_*` — conditional search, returns Option.
- `create_*` — insert, return created row.
- `update_*` — modify, return updated row. Never use `set_*`.
- `delete_*` — remove by ID.
