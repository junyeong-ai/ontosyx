# 0024 — `advisory_lock` for boot-time + cron singletons

**Status:** Accepted

**Date:** 2026-05-05

**Supersedes:** none — codifies the canonical coordination
pattern; pre-existing surfaces (prompt seed, cron sweeps)
were inconsistent and have been retrofitted.

## Context

Two classes of work on the platform need single-replica
serialisation:

1. **Boot-time seeders.** The `prompt_templates` table seeds
   from `prompts/*.toml` on first boot of a fresh DB. A
   multi-replica deploy that fires the seeder concurrently
   races on the UPSERT — sometimes the second replica's
   write lands and overwrites in-flight edits the first
   replica was about to apply, sometimes the unique-key
   conflict aborts both writers and leaves the table in a
   half-seeded state.

2. **Cron sweeps.** Stale-concept proposal sweeps,
   quality-baseline rolling-window writes, soft-delete
   compaction, draft-checkpoint cleanup — every cron task
   that mutates shared state needs exactly-once semantics
   per tick. With N replicas each running the cron, a sweep
   that writes 10K rows runs 10×N times, half of those
   ticking against rows another replica has already
   processed.

The first cuts of both classes used either no coordination
(seeded the table on every boot, hoping the UPSERT race was
benign) or magic-`i64` `pg_advisory_lock` keys inlined per
call-site. Both shapes drifted:

- One cron task registered key `0xC4F3_0001`; another wrote
  `0xC4F3_0002`; a third decided it didn't need a lock at all.
- The seeder used `pg_advisory_lock` directly without a
  released-on-error wrapper; a panic mid-seed left the lock
  held and every subsequent boot blocked.
- The matching test had to know "what i64 key did `cron_X`
  pick?" to verify the lock acquisition; refactors silently
  shifted keys.

## Decision

`ox_store::advisory_lock` (`crates/ox-store/src/advisory_lock.rs`)
is the canonical coordination layer for shared-write
serialisation. Two helpers:

- **`with_advisory_lock(pool, key, fut)`** — blocking. The
  caller awaits the lock; the future runs only after the
  lock is held, and the lock releases via RAII on success
  *or* panic. Boot-time seeders that must run exactly once
  per fresh DB (e.g. `ADVISORY_LOCK_PROMPT_SEED`).

- **`try_advisory_lock(pool, key, fut)`** — non-blocking.
  Returns `Ok(None)` when another holder has the lock; the
  caller silently skips. Cron singletons that should run on
  one replica per tick (`ADVISORY_LOCK_CRON_*`).

Lock keys are workspace-level constants in the `advisory_lock`
module:

```rust
pub const ADVISORY_LOCK_PROMPT_SEED: i64 = ...;
pub const ADVISORY_LOCK_CRON_STALE_PROPOSAL: i64 = ...;
pub const ADVISORY_LOCK_CRON_QUALITY_BASELINE: i64 = ...;
// ...
```

Inline magic `i64` literals in caller code are forbidden.
The `lock_constants_are_unique` test pins every constant
against collisions so a copy-paste typo in a new lock fails
CI immediately.

`CronTask::singleton_key()` is the trait hook cron tasks
override:

```rust
fn singleton_key(&self) -> Option<i64> {
    Some(ADVISORY_LOCK_CRON_<NAME>)
}
```

The scheduler wraps each `run_once` in `try_advisory_lock`
keyed on `singleton_key()`; tasks that override return
`None` keep the previous behaviour (every replica runs on
its own state — clarification evict, collaboration idle reap).

## Consequences

- **Boot-time seeders are deterministic.** No more
  half-seeded prompt tables. RAII release means panics
  during seed don't permanently block subsequent boots.
- **Cron sweeps run exactly once per tick.** The N-replica
  fan-out becomes a single writer + N-1 silent skips.
  Affected cron names that benefit:
  `archive_stale_drafts`, `delete_archived_drafts`, the
  quality-baseline sweep, draft-cluster-checkpoint
  cleanup, soft-delete compaction.
- **Test surface is uniform.** Tests that need to assert
  "this code path acquires the lock" call into
  `ox_store::advisory_lock::test_support` rather than
  re-implementing the wrapper.
- **Constants are linted.** `lock_constants_are_unique`
  runs on every CI; a duplicate i64 fails the test before
  it can land.

## Adoption

Currently registered (one constant per surface):

- `ADVISORY_LOCK_PROMPT_SEED` — boot-time prompt seeder.
- `ADVISORY_LOCK_CRON_STALE_PROPOSAL` —
  stale-concept-proposal sweep.
- `ADVISORY_LOCK_CRON_QUALITY_BASELINE` — quality
  rolling-window baseline writes.
- `ADVISORY_LOCK_CRON_DRAFT_CHECKPOINT_CLEANUP` —
  expired draft-cluster-checkpoint sweep.
- `ADVISORY_LOCK_CRON_SOFT_DELETE_COMPACTION` —
  audit-grace soft-delete compaction.
- `ADVISORY_LOCK_CRON_ARCHIVE_DRAFTS` —
  WIP draft archive sweep.
- `ADVISORY_LOCK_CRON_DELETE_ARCHIVED_DRAFTS` —
  permanent-delete sweep on archived drafts.

A new lock surface is "new constant + one call to
`with_` or `try_advisory_lock`"; copy-paste of an existing
i64 fails the uniqueness test, so the new constant is the
only path.

## Alternatives considered

- **Postgres `LOCK TABLE ... IN ACCESS EXCLUSIVE MODE`** —
  rejected. Coarser than needed (locks the whole table for
  the duration); breaks reads on the same table during
  the seed / sweep. Advisory locks are session-level and
  invisible to non-coordinating queries.
- **Redis-backed leader election** — rejected. Adds a
  coordination dependency that doesn't otherwise exist;
  Postgres is already in the dependency surface.
- **Magic-i64 keys per call-site** — rejected (described
  in Context above). Drift mode that produces silent
  collisions.
- **Zookeeper / etcd** — rejected for the same reason as
  Redis: extra dependency for a coordination problem
  Postgres can solve in one statement.

## References

- Memory entry: `feedback_advisory_lock_pattern.md`
- Memory entry: `feedback_cron_singleton_pattern.md`
- Primitive: `crates/ox-store/src/advisory_lock.rs`
- Test pin: `crates/ox-store/src/advisory_lock.rs::lock_constants_are_unique`
- Postgres advisory locks docs
