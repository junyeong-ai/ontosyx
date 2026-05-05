# 0012 — RLS enforcement contract: every store mutation requires explicit scope

**Status:** Accepted

**Date:** 2026-04-29

**Supersedes:** none

## Context

The Postgres store layer relies on PostgreSQL Row-Level Security
to enforce workspace isolation. Two task-locals
(`ox_store::WORKSPACE_ID`, `ox_store::SYSTEM_BYPASS`) feed
session variables (`app.workspace_id`, `app.system_bypass`) that
the RLS policies in `0001_schema.sql` read on every query.

The pre-B6 contract had a quiet failure mode: a mutating store
method called *outside* both a `WORKSPACE_ID.scope(...)` block
and a `SYSTEM_BYPASS.scope(true, ...)` block would acquire a
connection where neither session variable is set. RLS would then
deny the row at insert time (write paths) or return zero rows at
read time, leaving the caller to wonder why their write
"succeeded" with zero rows affected.

For reads this is the safe deny-all default — empty results are
correct when the caller is unauthenticated. For writes the
silence is dangerous: a background task that forgot to wrap
itself in `with_workspace` will look like it's working while
nothing reaches the database.

## Decision

Two changes make the contract explicit:

1. **`OxError::MissingContext { kind, message }`** — a new error
   variant for "a required scope or context was not set on the
   calling task." The variant is generic on `kind` so the same
   shape covers workspace, project, user, or any future scope
   axis without per-axis variants. (We considered
   `WorkspaceContextMissing` but rejected the per-axis form as
   too narrow — `kind: "workspace"` reads identically and the
   pattern extends.)

2. **`ox_store::require_workspace_context()`** — a guard helper
   every mutating store method calls at its entry. Returns
   `Ok(())` when either `WORKSPACE_ID` or `SYSTEM_BYPASS` is
   set, returns `OxError::MissingContext { kind: "workspace", … }`
   otherwise. Read-only methods don't have to call it; RLS
   already returns the safe empty result on missing context, and
   a missing-context read is often a feature (the OIDC startup
   path prefetches the workspace list before any user has
   authenticated).

The guard is opt-in to keep the migration cheap: future mutating
methods MUST call `require_workspace_context()?` at their top,
and the existing 43 store traits' methods migrate to the pattern
incrementally as we touch them. The guard's unit tests
(`postgres::context_guard_tests`) lock the contract:
in-`WORKSPACE_ID.scope` passes, in-`SYSTEM_BYPASS.scope` passes,
no scope returns `MissingContext`.

## Consequences

**Positive.**

- Silent zero-rows-affected writes become structured errors. A
  background task that forgot to wrap itself in `with_workspace`
  surfaces as a `MissingContext` 500 with a clear remedy in the
  message instead of a phantom success.
- The error variant generalises to project / user / future scopes
  without reshaping the API.
- The pre-existing `with_workspace` / `with_system_bypass`
  helpers stay the canonical entry points; the guard only
  enforces what was already required by convention.

**Negative.**

- Every new mutating store method needs a one-line opt-in. Code
  review must catch the case where a contributor adds a write
  method without the guard. Mitigation: `rls_enforcement.rs`
  already pins the policy-level invariants; adding a CI lint
  that flags `async fn (create|update|upsert|delete)_*` without
  a `require_workspace_context()` call is a reasonable next
  step but not in scope for this ADR.
- The opt-in (rather than blanket) approach means existing
  mutators don't all enforce the guard from day one. The risk
  is bounded — the existing pre-B6 behaviour stays in place
  for those callers (silent deny-all) and migrating each
  trait can land alongside its next intentional touch.

## How callers should use it

Mutating methods at the top:

```rust
async fn create_artifact(&self, …) -> OxResult<…> {
    ox_store::require_workspace_context()?;
    // … existing body …
}
```

Background tasks wrap their entry point:

```rust
PostgresStore::with_workspace(workspace_id, || async {
    store.create_ontology_draft(&draft).await
}).await
```

Cross-workspace tools (cleanup, migration, federation drift):

```rust
PostgresStore::with_system_bypass(|| async {
    store.list_workspaces_for_drift_check().await
}).await
```
