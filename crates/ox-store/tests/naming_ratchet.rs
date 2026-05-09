//! Naming-convention ratchet test.
//!
//! Catches new symbols that match patterns the workspace
//! conventions forbid:
//!
//! - **`*Lookup` (store-as-verb)** — pick the noun that names
//!   the trait or struct. Use `*Store` for the trait, plain
//!   `get_X` / `find_X` / `list_X` methods for the verbs.
//! - **`*Loop` (control-flow as noun)** — represent the loop's
//!   state as a typed `*Session` / `*Attempt` instead.
//! - **`*Manager`, `*Helper`** — vague suffix; pick the noun
//!   that says what the type IS, not what it does.
//! - **`pub async fn save_*` on store impls** — forbidden CRUD
//!   shape. Use `create_X` / `update_X` / `upsert_X` per the
//!   "Store methods" section in the workspace `CLAUDE.md`.
//!
//! Baseline at the time of writing (Φ8.5, 2026-05-08): zero
//! violations across the workspace. The test pins that baseline
//! — adding a new symbol that matches any of the forbidden
//! patterns trips the test, and the author either renames the
//! symbol or extends the explicit allow-list at the top of this
//! file with a recorded reason.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};

/// Type-name suffixes that the workspace convention forbids.
/// Whole-word match — a token that equals the suffix exactly is
/// not flagged (the suffix on its own is a legitimate one-word
/// noun, only its use *as a suffix* is the smell).
const FORBIDDEN_TYPE_SUFFIXES: &[&str] = &["Lookup", "Manager", "Helper", "Loop"];

/// Method-name prefix that collapses to the canonical
/// `create_X` / `update_X` shape. Only enforced inside
/// `crates/ox-store/src/postgres/` because that's the surface
/// the convention pins.
const FORBIDDEN_STORE_METHOD_PREFIX: &str = "save_";

/// Explicit allowance list. Each entry is keyed on the qualified
/// name ("crate-relative path/to/file.rs:Symbol") and carries the
/// reason. Empty at Φ8.5 baseline; populated only when a future
/// case demonstrates the suffix is genuinely the right name.
const ALLOWLIST: &[(&str, &str)] = &[];

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("workspace root reachable from ox-store")
        .to_path_buf()
}

fn collect_rs_files(root: &Path, out: &mut Vec<PathBuf>) {
    if !root.is_dir() {
        return;
    }
    let entries = match fs::read_dir(root) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            // Skip target / node_modules / .git / vendor and the
            // `tests/` subtree of every crate (test fixtures may
            // legitimately use names that production code cannot).
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if matches!(
                name,
                "target" | "node_modules" | ".git" | "vendor" | "tests"
            ) {
                continue;
            }
            collect_rs_files(&path, out);
        } else if path.extension().and_then(|s| s.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

/// Strip the leading visibility modifier (`pub`, `pub(crate)`,
/// `pub(super)`, `pub(in path::to)`) and any whitespace from a
/// trimmed line. Returns the rest of the line — used as the
/// anchor for `struct` / `trait` / `enum` keyword matching.
fn after_visibility(trimmed: &str) -> &str {
    let after_pub = match trimmed.strip_prefix("pub") {
        Some(s) => s,
        None => return trimmed,
    };
    // Reject `public_*` identifier — must be followed by ws or '('.
    let next_char = after_pub.chars().next();
    if !matches!(next_char, Some(c) if c.is_whitespace() || c == '(') {
        return trimmed;
    }
    let rest = after_pub.trim_start();
    if rest.starts_with('(') {
        match rest.find(')') {
            Some(end) => rest[end + 1..].trim_start(),
            None => rest,
        }
    } else {
        rest
    }
}

/// First contiguous identifier token at the start of `s`.
fn leading_ident(s: &str) -> &str {
    let end = s
        .char_indices()
        .find(|(_, c)| !(c.is_alphanumeric() || *c == '_'))
        .map(|(i, _)| i)
        .unwrap_or(s.len());
    &s[..end]
}

fn type_definition_violations(content: &str, file: &Path, root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let rel = file.strip_prefix(root).unwrap_or(file).display().to_string();
    for (idx, line) in content.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("//") {
            continue;
        }
        let after_pub = after_visibility(trimmed);
        for kw in ["struct ", "trait ", "enum "] {
            let after_kw = match after_pub.strip_prefix(kw) {
                Some(s) => s,
                None => continue,
            };
            let token = leading_ident(after_kw);
            if token.is_empty() {
                continue;
            }
            for suffix in FORBIDDEN_TYPE_SUFFIXES {
                if token.ends_with(suffix) && token.len() > suffix.len() {
                    let qualified = format!("{rel}:{token}");
                    if ALLOWLIST.iter().any(|(name, _)| *name == qualified) {
                        continue;
                    }
                    out.push(format!(
                        "{}:{}: forbidden type-name suffix `{}` in `{}`",
                        rel,
                        idx + 1,
                        suffix,
                        token
                    ));
                }
            }
        }
    }
    out
}

fn store_method_violations(content: &str, file: &Path, root: &Path) -> Vec<String> {
    let rel = file.strip_prefix(root).unwrap_or(file).display().to_string();
    if !rel.contains("ox-store/src/postgres/") {
        return Vec::new();
    }
    let mut out = Vec::new();
    for (idx, line) in content.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("//") {
            continue;
        }
        // Match `pub async fn save_*` or `async fn save_*` on impl
        // blocks. The `pub` form covers the trait-impl method;
        // the bare `async fn` covers private helpers that still
        // collapse to CRUD if they end up named `save_`.
        for kw in ["pub async fn ", "async fn ", "pub fn ", "fn "] {
            let Some(after) = trimmed.strip_prefix(kw) else {
                continue;
            };
            let token = leading_ident(after);
            if token.starts_with(FORBIDDEN_STORE_METHOD_PREFIX) {
                let qualified = format!("{rel}:{token}");
                if ALLOWLIST.iter().any(|(name, _)| *name == qualified) {
                    continue;
                }
                out.push(format!(
                    "{}:{}: forbidden store method prefix `save_` in `{}` \
                     (use `create_X` / `update_X` / `upsert_X` per ox-store/CLAUDE.md)",
                    rel,
                    idx + 1,
                    token
                ));
            }
        }
    }
    out
}

#[test]
fn no_forbidden_naming_patterns_in_workspace_sources() {
    let root = workspace_root();
    let crates_dir = root.join("crates");
    assert!(
        crates_dir.is_dir(),
        "crates/ directory not reachable from {}",
        root.display()
    );

    let mut files: Vec<PathBuf> = Vec::new();
    for entry in fs::read_dir(&crates_dir).expect("crates/ readable").flatten() {
        let crate_root = entry.path();
        let src = crate_root.join("src");
        collect_rs_files(&src, &mut files);
    }

    let mut violations: Vec<String> = Vec::new();
    for path in &files {
        let Ok(content) = fs::read_to_string(path) else {
            continue;
        };
        violations.extend(type_definition_violations(&content, path, &root));
        violations.extend(store_method_violations(&content, path, &root));
    }

    if !violations.is_empty() {
        panic!(
            "naming ratchet failed — {} violation(s):\n{}\n\n\
             Forbidden type suffixes: {:?}\n\
             Forbidden store-method prefix: `{}`\n\
             Add an entry to ALLOWLIST in this file only if the symbol \
             is genuinely the right name and the convention exception is recorded.",
            violations.len(),
            violations.join("\n"),
            FORBIDDEN_TYPE_SUFFIXES,
            FORBIDDEN_STORE_METHOD_PREFIX
        );
    }
}

#[test]
fn after_visibility_strips_pub_modifiers() {
    assert_eq!(after_visibility("pub struct Foo"), "struct Foo");
    assert_eq!(after_visibility("pub(crate) struct Foo"), "struct Foo");
    assert_eq!(after_visibility("pub(super) trait Bar"), "trait Bar");
    assert_eq!(after_visibility("pub(in crate::x) enum Baz"), "enum Baz");
    assert_eq!(after_visibility("struct Naked"), "struct Naked");
    // `public_method` is not a visibility modifier — leading ident
    // happens to start with `pub` but the next char is `_`.
    assert_eq!(after_visibility("public_method()"), "public_method()");
}

#[test]
fn leading_ident_stops_at_non_word_boundary() {
    assert_eq!(leading_ident("FooLookup<T>"), "FooLookup");
    assert_eq!(leading_ident("Bar { ... }"), "Bar");
    assert_eq!(leading_ident("save_user(&self)"), "save_user");
    assert_eq!(leading_ident(""), "");
}
