//! Migration immutability gate.
//!
//! `sqlx-migrate` records the SHA-256 of every applied migration in
//! `_sqlx_migrations.checksum`. Editing a migration file that's
//! already applied to a deployment is therefore a deployment-
//! breaking operation — the next `migrate` call detects the
//! checksum mismatch and refuses to start. Mirroring the platform-
//! standard advice: migrations are append-only.
//!
//! ## Self-bootstrapping baseline
//!
//! The pinned hashes live in `tests/migration_baseline.json` —
//! a git-tracked JSON file mapping `<filename>.sql → sha256(hex)`.
//! Two flows:
//!
//! - **Default**: every PR runs `cargo test --test migration_immutability`
//!   and the gate compares each `migrations/NNNN_*.sql` file against
//!   its baseline entry. A hash mismatch means a sealed file was
//!   edited (forbidden); a missing entry means a new migration
//!   landed without baseline registration.
//!
//! - **Bootstrap**: when adding a new migration, set
//!   `OX_UPDATE_MIGRATION_BASELINE=1` and re-run the test. The
//!   baseline file is regenerated from the current state of
//!   `migrations/` and the test passes. Commit the baseline diff
//!   alongside the new SQL file. The diff in the PR documents the
//!   deliberate registration; review can sanity-check the new entry
//!   without the author hand-copying a hex hash.
//!
//! Same pattern as `web/scripts/heading-primitive-audit.mjs`'s
//! baseline ratchet, kept in sync so contributors learn one mental
//! model for "ratcheted invariant + JSON baseline" across the repo.
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
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

const MIGRATIONS_DIR: &str = "migrations";
const BASELINE_FILE: &str = "tests/migration_baseline.json";
const UPDATE_ENV: &str = "OX_UPDATE_MIGRATION_BASELINE";

fn manifest_path(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(rel)
}

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

/// Hash every `NNNN_*.sql` file under `migrations/`. Returns a
/// sorted map for stable JSON output. The directory walk is the
/// single source of truth for "which files exist"; the baseline
/// file is the source of truth for "what their hashes were when
/// last sanctioned".
fn current_hashes() -> BTreeMap<String, String> {
    let dir = manifest_path(MIGRATIONS_DIR);
    let entries = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("read_dir {} failed: {e}", dir.display()));
    let mut out: BTreeMap<String, String> = BTreeMap::new();
    for entry in entries {
        let entry = entry.expect("dir entry");
        let name = entry.file_name();
        let name_str = name.to_string_lossy().into_owned();
        if !looks_like_migration(&name_str) {
            // Strays are surfaced by the sibling test —
            // `migrations_directory_has_no_strays` — so skip
            // silently here rather than mixing the two failure
            // modes into one assertion.
            continue;
        }
        out.insert(name_str, hash_file(&entry.path()));
    }
    out
}

/// Read the baseline JSON. The test's hard-fails on parse error
/// rather than treating it as "no baseline" — a missing file is
/// distinct from a corrupt one (the former invites bootstrap, the
/// latter is a code-review error).
fn read_baseline() -> BTreeMap<String, String> {
    let path = manifest_path(BASELINE_FILE);
    let raw = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "Reading {} failed: {e}\n\
             First-time setup: create the file with `{{}}` and re-run \
             with {UPDATE_ENV}=1 to populate it from the current \
             migrations/ directory.",
            path.display()
        )
    });
    serde_json::from_str(&raw).unwrap_or_else(|e| {
        panic!(
            "Parsing {} as JSON failed: {e}\n\
             The baseline file is a `{{ \"<filename>.sql\": \"<sha256-hex>\" }}` \
             map; restore the file or regenerate via {UPDATE_ENV}=1.",
            path.display()
        )
    })
}

/// Write the baseline JSON. Pretty-printed with two-space indent
/// and a trailing newline so the file diff is readable when a PR
/// adds a migration.
fn write_baseline(map: &BTreeMap<String, String>) {
    let path = manifest_path(BASELINE_FILE);
    // `serde_json::to_string_pretty` renders BTreeMap entries in
    // key order, matching the file's existing layout. The trailing
    // newline keeps the file POSIX-clean.
    let mut json = serde_json::to_string_pretty(map).expect("serialize baseline");
    json.push('\n');
    std::fs::write(&path, json)
        .unwrap_or_else(|e| panic!("write {} failed: {e}", path.display()));
}

#[test]
fn historical_migrations_are_immutable() {
    let current = current_hashes();
    let bootstrap = std::env::var(UPDATE_ENV).is_ok();

    if bootstrap {
        // Bootstrap mode: write the current state as the new
        // baseline, then succeed. The PR diff records the change
        // so a reviewer sees exactly what was registered.
        write_baseline(&current);
        return;
    }

    let expected = read_baseline();

    let mut mismatches: Vec<String> = Vec::new();
    let mut missing_in_baseline: Vec<String> = Vec::new();
    let mut missing_files: Vec<String> = Vec::new();

    // Compare existing files to the baseline.
    for (name, got) in &current {
        match expected.get(name) {
            None => missing_in_baseline.push(name.clone()),
            Some(want) if want != got => mismatches.push(format!(
                "  {name}\n    expected: {want}\n    actual:   {got}"
            )),
            _ => {}
        }
    }
    // Catch baseline entries with no corresponding file (deletion
    // of a sealed migration is also forbidden).
    for name in expected.keys() {
        if !current.contains_key(name) {
            missing_files.push(name.clone());
        }
    }

    if mismatches.is_empty()
        && missing_in_baseline.is_empty()
        && missing_files.is_empty()
    {
        return;
    }

    let mut msg = String::from("Migration immutability gate\n\n");
    if !mismatches.is_empty() {
        msg.push_str(
            "Hash mismatches (a sealed migration file was edited — \
             forbidden, schema evolution lands as a NEW file):\n",
        );
        for line in &mismatches {
            msg.push_str(line);
            msg.push('\n');
        }
        msg.push('\n');
    }
    if !missing_in_baseline.is_empty() {
        msg.push_str(
            "New migration(s) not yet registered in the baseline:\n",
        );
        for name in &missing_in_baseline {
            msg.push_str("  ");
            msg.push_str(name);
            msg.push('\n');
        }
        msg.push('\n');
    }
    if !missing_files.is_empty() {
        msg.push_str(
            "Files in the baseline but missing from migrations/ \
             (deletion of a sealed migration is forbidden):\n",
        );
        for name in &missing_files {
            msg.push_str("  ");
            msg.push_str(name);
            msg.push('\n');
        }
        msg.push('\n');
    }
    msg.push_str(&format!(
        "Remediation:\n  \
         - For a NEW migration: re-run with `{UPDATE_ENV}=1 cargo test \
         --test migration_immutability` to register it in the baseline, \
         then commit the baseline diff alongside the SQL file.\n  \
         - For an UNINTENDED edit: revert the change and add a fresh \
         `NNNN_<focus>.sql` instead.\n  \
         - For a deliberate cosmetic restructure (rare — only when sqlx \
         migration metadata is unaffected): re-run with `{UPDATE_ENV}=1` \
         and explicitly call out the rationale in the PR description.\n"
    ));
    panic!("{msg}");
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

/// Every file in `migrations/` is either pinned in the baseline
/// (historical, immutable) or matches a `NNNN_*.sql` naming pattern
/// for a future migration. Catches accidental scratch files
/// (`0001_schema.sql.bak`, `temp.sql`, etc.) before they confuse
/// `sqlx-migrate`.
#[test]
fn migrations_directory_has_no_strays() {
    let dir = manifest_path(MIGRATIONS_DIR);
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
