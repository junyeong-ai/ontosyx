//! Migration immutability gate.
//!
//! Historical migrations are append-only. Once a migration file
//! lands on `main`, its bytes are frozen — schema corrections
//! and additive changes go in a *new* migration with the next
//! `NNNN_` prefix, never as edits to a prior file. This test
//! enforces that contract by pinning the SHA-256 of every
//! migration file in the workspace and failing fast on any
//! drift.
//!
//! ## Why this matters
//!
//! `sqlx::migrate!` resolves files at compile time and runs them
//! in lexicographic order on first connection. A silent edit to
//! a baseline file produces three classes of failure:
//!
//! 1. **Replicas diverge** — a deploy that re-runs `migrate()`
//!    on a fresh DB sees the new bytes, while existing
//!    deployments retain the original schema. Identical app
//!    versions sit on incompatible schemas.
//! 2. **Test fixtures drift** — an integration test author who
//!    edits 0001 to add a column passes locally because their
//!    test DB is freshly migrated; CI passes for the same
//!    reason. The bug surfaces only on staging where the
//!    migration was already applied weeks ago.
//! 3. **Bisects break** — `git checkout` of a prior commit no
//!    longer round-trips through migrations because the file's
//!    historical hash conflicts with sqlx's `_sqlx_migrations`
//!    checksum.
//!
//! ## Contract
//!
//! - Every file in `migrations/*.sql` MUST appear in
//!   [`PINNED_MIGRATIONS`] with its current SHA-256.
//! - A pinned file MUST exist on disk (deletes forbidden).
//! - A pinned file's bytes MUST match the recorded hash
//!   (edits forbidden).
//!
//! ## Adding a migration
//!
//! 1. Create `migrations/NNNN_<verb>.sql` with the next
//!    available `NNNN`.
//! 2. Run this test. It fails with a copy-paste-ready
//!    `(filename, sha256)` tuple for the new file.
//! 3. Append the tuple to [`PINNED_MIGRATIONS`].
//! 4. Re-run; the test passes.
//!
//! ## Correcting a baseline (rare)
//!
//! If a historical migration *must* change (e.g. a typo that
//! breaks every fresh-DB boot, caught before the change has
//! propagated to production), the maintainer:
//!
//! 1. Edits the file.
//! 2. Updates the pin to the new hash.
//! 3. Documents the change in the commit message AND
//!    coordinates a coordinated re-deploy of every replica
//!    (sqlx will detect the checksum mismatch on already-
//!    migrated DBs and refuse to start).
//!
//! The default workflow — and the only safe one — is to ship
//! a new file.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

/// SHA-256 pin for every migration file the workspace ships.
/// Entries are checked into source so review surfaces every
/// change as a deliberate diff, and `cargo test --test
/// migration_immutability` fails the moment a file's bytes
/// drift from its pin.
const PINNED_MIGRATIONS: &[(&str, &str)] = &[
    (
        "0001_schema.sql",
        "877deaae0b01df8f7aa720c2ba55437236d231996badeefb7ecb2e3178913c30",
    ),
    (
        "0002_source_contracts.sql",
        "ae2ee152f49b9852d4f35e7fc15bd5047330329a4b1f91df1b9ffcb9c37261f5",
    ),
    (
        "0003_verified_query_embedding.sql",
        "16054c5740fa8ab20d5a45f6d0ab5ff65f6a8a8107bd55af199432fe46af121e",
    ),
    (
        "0004_knowledge_hybrid_indexes.sql",
        "6ea7d928059cf58bcc74c26bfdef6afd012b2a58745b103200ef9c30280f7476",
    ),
    (
        "0005_community_summary_embedding.sql",
        "6d94fff9e9d0ec1e87a373d64239dc9b4824db7498bcbb09afc8adc8fd415b40",
    ),
    (
        "0006_drop_agent_session_model_config.sql",
        "e2f423094aedceb3511bb857ac9c0db6fd886adfdc3681af4e12657bdea3b8d3",
    ),
    (
        "0007_model_prices_cache_creation_tariff.sql",
        "a4852248702e19f8d0f143e11d89a179f5789289c2e96b3b9819a957b24f9b52",
    ),
];

#[test]
fn migrations_are_immutable() {
    let dir = migrations_dir();
    let on_disk = read_migration_hashes(&dir);
    let pinned: BTreeMap<&str, &str> = PINNED_MIGRATIONS.iter().copied().collect();

    let mut violations: Vec<String> = Vec::new();

    for (name, expected) in &pinned {
        match on_disk.get(*name) {
            Some(actual) if actual == expected => {}
            Some(actual) => violations.push(format!(
                "MODIFIED: {name}\n      pinned hash: {expected}\n      actual hash: {actual}\n      \
                 New schema changes go in a NEW migration file, not edits to {name}.",
            )),
            None => violations.push(format!(
                "DELETED: {name}\n      pinned hash: {expected}\n      \
                 Migration files are append-only; restore the file or remove the pin only \
                 alongside a coordinated re-deploy.",
            )),
        }
    }

    let pinned_names: BTreeSet<&str> = pinned.keys().copied().collect();
    for (name, actual) in &on_disk {
        if !pinned_names.contains(name.as_str()) {
            violations.push(format!(
                "UNPINNED: {name}\n      hash: {actual}\n      \
                 Append the pin to PINNED_MIGRATIONS in this file:\n      \
                 (\n          \"{name}\",\n          \"{actual}\",\n      ),",
            ));
        }
    }

    assert!(
        violations.is_empty(),
        "migration immutability gate failed — see crates/ox-store/tests/migration_immutability.rs \
         for the contract.\n\n{}",
        violations.join("\n\n"),
    );
}

#[test]
fn pinned_migrations_are_unique_and_sorted() {
    let mut prev: Option<&str> = None;
    for (name, _) in PINNED_MIGRATIONS {
        if let Some(p) = prev {
            assert!(
                p < *name,
                "PINNED_MIGRATIONS is not strictly sorted: '{p}' is not less than '{name}'.",
            );
        }
        prev = Some(name);
    }
}

#[test]
fn pinned_hashes_are_lowercase_hex_64_chars() {
    for (name, hash) in PINNED_MIGRATIONS {
        assert_eq!(
            hash.len(),
            64,
            "{name}: expected 64-char SHA-256 hex string, got {} chars",
            hash.len(),
        );
        assert!(
            hash.chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
            "{name}: pinned hash must be lowercase hex (got '{hash}')",
        );
    }
}

fn migrations_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations")
}

fn read_migration_hashes(dir: &Path) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let entries =
        fs::read_dir(dir).unwrap_or_else(|err| panic!("read_dir({}) failed: {err}", dir.display()));
    for entry in entries {
        let entry = entry.expect("read_dir entry");
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("sql") {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .map(str::to_string)
            .expect("migration filename is utf-8");
        let bytes =
            fs::read(&path).unwrap_or_else(|err| panic!("read({}) failed: {err}", path.display()));
        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        out.insert(name, hex::encode(hasher.finalize()));
    }
    out
}
