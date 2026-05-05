# 0025 — Migration immutability + SHA-pinned baseline

**Status:** Accepted

**Date:** 2026-05-05

**Supersedes:** none — codifies the canonical contract; the
`migration_immutability` test was added retroactively after a
historical-edit incident.

## Context

`crates/ox-store/migrations/NNNN_*.sql` files are append-only
by construction: sqlx-migrate records the SHA-256 of every
applied migration in the `_sqlx_migrations.checksum` column,
and editing a historical file fails the checksum check on the
next deploy.

Even with the runtime check, two failure modes were observed
in development:

1. **Local DBs accept the edit.** A developer editing
   `0001_schema.sql` to "fix a typo in a column name" never
   re-ran sqlx-migrate against a fresh DB; the change passed
   their local development cycle and only failed on the next
   colleague who pulled.

2. **Editor backups slip in.** `0001_schema.sql.bak`,
   `0001_schema.sql~`, `.0001_schema.sql.swp` files left
   behind by editors get globbed into sqlx-migrate's
   discovery pass and cause "duplicate migration version"
   panics on the next start.

Both failure modes need to fail at PR time, not at
deploy / start time.

## Decision

Two CI-enforced contracts on `crates/ox-store/migrations/`:

### `migration_immutability` test

The test pins every historical file's hash through
`tests/migration_baseline.json` — a git-tracked map of
`<filename>.sql → sha256(hex)`. Editing a sealed file fails
the test with the expected vs. actual hash printed for
diagnosis:

```
Migration 0001_schema.sql changed hash:
  expected: a1b2c3...
  actual:   d4e5f6...
```

Adding a new migration (or doing a deliberate baseline
regeneration after a `Project → OntologyDraft`-style sweep)
is a one-liner:

```
OX_UPDATE_MIGRATION_BASELINE=1 cargo test --test migration_immutability
```

The baseline regenerates from the current state of
`migrations/`, the test passes, and the resulting
`migration_baseline.json` diff gets committed alongside the
new SQL file. Hand-copying hex hashes is forbidden — the
deliberate registration is still visible in the PR diff via
the JSON change.

The same self-bootstrapping baseline pattern is used
elsewhere in the repo (`web/scripts/heading-primitive-audit.mjs`,
`web/scripts/contrast-audit.mjs`); one mental model for
"ratcheted invariant + JSON baseline" across the platform.

### `migrations_directory_has_no_strays` test

The test rejects anything that doesn't match
`^\d{4}_[a-z0-9_]+\.sql$` from the `migrations/` directory.
Catches editor backups (`*.sql.bak` / `*.sql~`) and rename
leftovers (`0005_design_project_parent_version.sql.orig`)
before they confuse sqlx-migrate.

## Consequences

- **Schema drift fails PR, not deploy.** A historical edit
  surfaces in CI immediately; the developer can either back
  out the change or run the explicit baseline regen
  command.
- **Editor noise fails CI.** `*.sql.bak` / `*.sql~` files
  surface immediately rather than blowing up in production
  on the next start.
- **Lexicon migrations have an escape valve.** When a sweep
  needs to rewrite historical migrations cleanly (the
  `Project → OntologyDraft` lexicon migration in commit
  566c49c, the `project_id → ontology_draft_id` column
  rename), the deliberate baseline regen is one command
  with one diff that the reviewer scans.
- **No "split the monolith" temptation.** Splitting
  `0001_schema.sql` into per-domain files would mutate
  every historical hash and break every existing
  deployment. The sealed monolith documents the v0
  baseline; new domains land as fresh
  `NNNN_<focus>.sql` files.

## Adoption

Every migration since the test landed has gone through this
shape:

- Add `NNNN_<focus>.sql` (where `NNNN == max(existing) + 1`).
- Run `OX_UPDATE_MIGRATION_BASELINE=1 cargo test --test
  migration_immutability` to capture the new file's hash.
- Commit the SQL + the `migration_baseline.json` diff
  together.

Recent uses:

- `0008_ontology_draft_committed_version.sql` (Phase 1
  `committed_version_id` snapshot link).
- The earlier `Project → OntologyDraft` sweep regenerated
  the baseline because the rename touched historical files
  intentionally.

## Alternatives considered

- **Runtime-only checksum check** — rejected. Failure mode
  surfaces on deploy / start, not on PR; the developer who
  introduced the drift is already off-shift.
- **Read-only filesystem permissions on `migrations/`** —
  rejected. Doesn't survive `git pull`; doesn't catch the
  baseline-regen footgun.
- **Squash old migrations periodically** — rejected.
  Squashing rewrites every existing deployment's checksum
  history; the sealed-monolith pattern is the platform's
  answer.

## References

- Memory entry: `feedback_migration_immutability_gate.md`
- Test pin: `crates/ox-store/tests/migration_immutability.rs`
- Test pin: `crates/ox-store/tests/migrations_directory_has_no_strays.rs`
- Baseline file: `crates/ox-store/tests/migration_baseline.json`
- Sister pattern: `web/scripts/heading-primitive-audit.mjs`
- Sister pattern: `web/scripts/contrast-audit.mjs`
