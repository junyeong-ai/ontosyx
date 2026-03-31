# ox-store

PostgreSQL persistence with Row-Level Security.

## Adding a New Store Trait

1. Define the trait in `store.rs` with async methods.
2. Add it to the `Store` supertrait (both trait def and blanket impl).
3. Implement in `postgres.rs`.
4. Re-export from `lib.rs`.

## Migration Conventions

- File: `migrations/NNNN_description.sql` (sequential numbering).
- Always add RLS policies for workspace-scoped tables.
- Use `DOUBLE PRECISION` for monetary fields (not `NUMERIC` — sqlx maps NUMERIC to Decimal, not f64).
- Migrations auto-run on server start via `pg_store.migrate()`.

## Method Naming

- `list_*` — return Vec, cursor-paginated.
- `get_*` — return single item by ID.
- `find_*` — conditional search, returns Option.
- `create_*` — insert, return created row.
- `update_*` — modify, return updated row. Never use `set_*`.
- `delete_*` — remove by ID.
