//! Migration immutability gate.
//!
//! `sqlx-migrate` records the SHA-256 of every applied migration in
//! `_sqlx_migrations.checksum`. Editing a migration file that's
//! already applied to a deployment is therefore a deployment-
//! breaking operation — the next `migrate` call detects the
//! checksum mismatch and refuses to start. Mirroring the platform-
//! standard advice: migrations are append-only.
//!
//! This test pins the SHA-256 of each historical migration file in
//! source. A future PR that edits one fails the test and forces the
//! author to either:
//!
//! - Revert the edit and add a new migration file instead (the
//!   right answer for schema evolution), OR
//! - Update the pinned hash explicitly (the rare answer when a
//!   migration file is structurally identical but cosmetically
//!   reformatted — e.g. trailing-whitespace cleanup; the explicit
//!   update marks the change as deliberate).
//!
//! The test is intentionally not gated on a database fixture: it
//! reads files off disk and compares hashes. Runs in the default
//! `cargo test` so every PR sees the gate.
//!
//! Why not split `0001_schema.sql`? It would mutate every
//! historical hash, break every existing deployment on the next
//! migration sync, and offers no operational benefit — sqlx
//! already runs migrations in lexicographic order, the file-size
//! cost is irrelevant on disk, and the sealed monolith documents
//! the v0 baseline cleanly. New domains land as fresh
//! `NNNN_<focus>.sql` files; the existing files stay frozen.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeMap;
use std::path::Path;

use sha2::{Digest, Sha256};

const MIGRATIONS_DIR: &str = "migrations";

fn hash_file(path: &Path) -> String {
    let bytes = std::fs::read(path)
        .unwrap_or_else(|e| panic!("read {} failed: {e}", path.display()));
    let digest = Sha256::digest(&bytes);
    digest.iter().fold(String::with_capacity(64), |mut out, b| {
        use std::fmt::Write as _;
        let _ = write!(&mut out, "{b:02x}");
        out
    })
}

/// Pinned SHA-256 of every applied historical migration. The
/// constant is the source of truth — adding a new migration file
/// extends this map; editing an existing file changes its hash and
/// must be paired with an explicit update here.
fn expected_hashes() -> BTreeMap<&'static str, &'static str> {
    let mut m = BTreeMap::new();
    m.insert(
        "0001_schema.sql",
        "71690e1595fae3218e83feea6947223910ce7f60bc52317d2fb78c67bd2c4431",
    );
    m.insert(
        "0002_draft_cluster_checkpoints.sql",
        "546872ebfdef710d12f826f51ab0e134d31e9e5a821089a309027929bd8a2698",
    );
    m.insert(
        "0003_analysis_scope.sql",
        "6c88677d7e99255ebe0414329be6c7b36e7ecb9cc720b6e19015195ca8db54db",
    );
    m.insert(
        "0004_workspace_isolation.sql",
        "44f06b792167812af1fb8e11c2f878cff7a9dcfa53bfb2a670aac690ace00569",
    );
    m
}

#[test]
fn historical_migrations_are_immutable() {
    // The expected hashes are placeholders below — first run of
    // this test will fail and report the actual hashes. Update the
    // map to those values, commit, and the gate is armed: future
    // edits surface as a hash mismatch with a clear remediation
    // hint pointing at the test file itself.
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(MIGRATIONS_DIR);
    let expected = expected_hashes();

    let mut mismatches: Vec<String> = Vec::new();
    let mut missing_files: Vec<String> = Vec::new();
    for (name, want) in &expected {
        let path = dir.join(name);
        if !path.exists() {
            missing_files.push((*name).to_string());
            continue;
        }
        let got = hash_file(&path);
        if got != *want {
            mismatches.push(format!(
                "  {name}\n    expected: {want}\n    actual:   {got}"
            ));
        }
    }

    // Surface the actual hashes when the gate is freshly armed —
    // the placeholder values in the map intentionally don't match,
    // so the first CI run prints the values to copy-paste in.
    if !mismatches.is_empty() || !missing_files.is_empty() {
        let mut msg = String::from(
            "Migration immutability gate\n\n\
             Either a historical migration was edited (forbidden — \
             schema evolution lands as a NEW file) or the pinned hashes \
             in `tests/migration_immutability.rs` need to be updated \
             after a deliberate restructure.\n\n",
        );
        if !mismatches.is_empty() {
            msg.push_str("Hash mismatches:\n");
            for line in &mismatches {
                msg.push_str(line);
                msg.push('\n');
            }
        }
        if !missing_files.is_empty() {
            msg.push_str("\nMissing files (deletion forbidden):\n");
            for f in &missing_files {
                msg.push_str(&format!("  {f}\n"));
            }
        }
        msg.push_str(
            "\nThe right move for schema evolution is `NNNN_<focus>.sql` \
             with N == max(existing) + 1. Append-only is the contract \
             sqlx-migrate enforces at runtime — failing this test now \
             prevents a deployment-breaking checksum mismatch later.",
        );
        panic!("{msg}");
    }
}

/// `sqlx-migrate` reads files matching `^\d{4}_.*\.sql$`. Anything
/// else in `migrations/` is either an accidental editor backup or
/// a half-finished rename — surface both before they confuse
/// future deploys.
fn looks_like_migration(name: &str) -> bool {
    let bytes = name.as_bytes();
    if bytes.len() < 6 {
        return false;
    }
    if !bytes[..4].iter().all(|b| b.is_ascii_digit()) {
        return false;
    }
    if bytes[4] != b'_' {
        return false;
    }
    if !name.ends_with(".sql") {
        return false;
    }
    // Body between the underscore and `.sql` is `[a-z0-9_]+` — keeps
    // the stem readable in `_sqlx_migrations.description`.
    name[5..name.len() - 4]
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// Every file in `migrations/` is either pinned in `expected_hashes`
/// (historical, immutable) or matches a `NNNN_*.sql` naming pattern
/// for a future migration. Catches accidental scratch files
/// (`0001_schema.sql.bak`, `temp.sql`, etc.) before they confuse
/// `sqlx-migrate`.
#[test]
fn migrations_directory_has_no_strays() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(MIGRATIONS_DIR);
    let entries = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("read_dir {} failed: {e}", dir.display()));

    let mut strays: Vec<String> = Vec::new();
    for entry in entries {
        let entry = entry.expect("dir entry");
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if !looks_like_migration(&name_str) {
            strays.push(name_str.into_owned());
        }
    }
    strays.sort();

    assert!(
        strays.is_empty(),
        "Stray files in migrations/ (sqlx-migrate ignores non-NNNN_*.sql \
         files but a stray editor backup or rename leftover indicates \
         a half-finished refactor): {strays:?}",
    );
}
