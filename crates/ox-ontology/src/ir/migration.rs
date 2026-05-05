//! `OntologyIR` JSONB schema migration pipeline.
//!
//! Persisted IR rows are JSONB; the in-memory struct evolves
//! independently. Without migration, every shape change is a
//! production hazard:
//!
//! - **Add a field** with `#[serde(default)]` — old rows load with
//!   the default. Safe but silently lossy if the new field carries
//!   meaning that should have been derived from the old shape.
//! - **Rename or restructure** — old rows fail to deserialise (or
//!   silently drop fields with `deny_unknown_fields = false`). Either
//!   path corrupts production data on read.
//!
//! This module fixes the structural-change case. Every persisted row
//! carries `schema_version`; the deserialiser routes through
//! [`migrate_to_current`] before the typed `OntologyIR` decode, so
//! old payloads pass through a chain of [`IrMigration`] steps that
//! transform their JSON shape forward to the current version.
//!
//! ## Adding a new migration
//!
//! When bumping [`super::ONTOLOGY_IR_SCHEMA_VERSION`] from `N` to
//! `N+1`:
//!
//! 1. Create `migration/v{N}_to_v{N+1}.rs` with a struct implementing
//!    [`IrMigration`].
//! 2. Append the struct to [`MIGRATIONS`] (this file). The chain
//!    test `migration_chain_is_continuous` fails the build if the
//!    chain has a gap.
//! 3. Add a fixture test: a JSON blob in v{N} shape, run through
//!    [`migrate_to_current`], assert the post-image matches the v{N+1}
//!    expected shape.
//!
//! The current build registers migrations for every historical
//! schema bump that needs one. Additive-only bumps (a new
//! `Vec<...>` collection guarded by `#[serde(default)]`) need no
//! entry — `serde` handles them transparently.

use ox_core::error::{OxError, OxResult};
use serde_json::Value;

use super::ONTOLOGY_IR_SCHEMA_VERSION;

mod v4_to_v5;

/// One step in the migration chain. Each implementor takes a JSON
/// payload tagged at `from_version` and returns the same logical
/// content reshaped to `to_version`. Implementations stay focused
/// on the exact diff between two consecutive versions; the pipeline
/// composes them.
pub trait IrMigration: Send + Sync {
    /// The version this migration transforms FROM.
    fn from_version(&self) -> u32;

    /// The version this migration transforms TO. Must be
    /// `from_version() + 1` — the chain test enforces consecutive
    /// numbering so a missing intermediate fails CI rather than
    /// silently degrading older data.
    fn to_version(&self) -> u32;

    /// Apply the transformation. Caller passes a `Value` already
    /// guaranteed to be `Object`-shaped at the IR root; the helper
    /// `as_object_mut` in this module asserts that and surfaces a
    /// typed error if some upstream caller passes garbage.
    fn migrate(&self, value: Value) -> OxResult<Value>;
}

/// Registered migration steps in order. Each successive step's
/// `from_version` must equal the previous step's `to_version`;
/// `migration_chain_is_continuous` enforces this at test time.
fn migrations() -> Vec<Box<dyn IrMigration>> {
    vec![Box::new(v4_to_v5::Migration)]
}

/// Walk from `payload`'s declared `schema_version` to
/// [`ONTOLOGY_IR_SCHEMA_VERSION`]. Payloads without an explicit
/// `schema_version` are treated as the current version (they
/// originated from a build that already wrote the field, just
/// lost it through a serializer that elides defaults).
///
/// Rejects payloads tagged with a version *newer* than the build
/// supports — silently downgrading would corrupt fields the build
/// can't represent.
///
/// Versions in the gap between consecutive registered migrations
/// are treated as additive-only: the payload passes through
/// unchanged at decode time (every new field carries
/// `#[serde(default)]`, so missing collections come back empty).
/// Structural changes register an explicit [`IrMigration`] in
/// [`migrations`]; everything else just walks the version tag
/// forward.
pub fn migrate_to_current(mut value: Value) -> OxResult<Value> {
    let from = read_schema_version(&value);
    if from > ONTOLOGY_IR_SCHEMA_VERSION {
        return Err(OxError::Validation {
            field: "schema_version".to_string(),
            message: format!(
                "OntologyIR schema_version {} is newer than this build supports (max {}). \
                 Upgrade the server or export/import through a compatible version.",
                from, ONTOLOGY_IR_SCHEMA_VERSION,
            ),
        });
    }
    if from == ONTOLOGY_IR_SCHEMA_VERSION {
        return Ok(value);
    }
    for migration in migrations() {
        if migration.from_version() < from {
            // Already past this step.
            continue;
        }
        if migration.from_version() == from {
            // The payload's current tag matches this step's input —
            // run the structural transformation.
            value = migration.migrate(value)?;
        }
        // Stamp the post-image with the migration's `to_version`
        // regardless of whether a transformation ran. Additive-only
        // gaps (no registered migration at this `from`) just walk
        // the version tag forward — the structural decode handles
        // missing fields via `#[serde(default)]`.
        if let Some(obj) = value.as_object_mut() {
            obj.insert(
                "schema_version".to_string(),
                Value::Number(serde_json::Number::from(migration.to_version())),
            );
        }
    }
    // Stamp the final tag in case the registered chain doesn't
    // reach CURRENT (additive-only versions on the trailing edge).
    if let Some(obj) = value.as_object_mut() {
        obj.insert(
            "schema_version".to_string(),
            Value::Number(serde_json::Number::from(ONTOLOGY_IR_SCHEMA_VERSION)),
        );
    }
    Ok(value)
}

/// Read the `schema_version` tag from a JSONB payload.
/// Defaults to [`ONTOLOGY_IR_SCHEMA_VERSION`] when absent — payloads
/// written by builds that elided the field were always at the
/// current shape on the writing side.
fn read_schema_version(value: &Value) -> u32 {
    value
        .get("schema_version")
        .and_then(|v| v.as_u64())
        .map(|v| v as u32)
        .unwrap_or(ONTOLOGY_IR_SCHEMA_VERSION)
}

/// Type-safe `as_object_mut` for migrations — they always operate
/// on the IR root, which is always a JSON object. A non-object
/// payload at this layer is a bug somewhere upstream; surface it
/// rather than panicking.
pub(crate) fn as_object_mut(
    value: &mut Value,
) -> OxResult<&mut serde_json::Map<String, Value>> {
    value.as_object_mut().ok_or_else(|| OxError::Validation {
        field: "ontology_ir".to_string(),
        message: "OntologyIR JSON root must be an object".to_string(),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Every registered migration's `to_version` must equal the
    /// next migration's `from_version`. Catches "we forgot the
    /// v3→v4 step but added v4→v5" the moment it lands.
    #[test]
    fn migration_chain_is_continuous() {
        let chain = migrations();
        for w in chain.windows(2) {
            assert_eq!(
                w[0].to_version(),
                w[1].from_version(),
                "migration chain gap: {} ends at {}, next starts at {}",
                std::any::type_name_of_val(&*w[0]),
                w[0].to_version(),
                w[1].from_version(),
            );
        }
        if let Some(last) = chain.last() {
            assert_eq!(
                last.to_version(),
                ONTOLOGY_IR_SCHEMA_VERSION,
                "last migration ends at {}, but ONTOLOGY_IR_SCHEMA_VERSION is {}",
                last.to_version(),
                ONTOLOGY_IR_SCHEMA_VERSION,
            );
        }
    }

    /// Each migration's `to_version` must equal `from_version + 1`.
    /// Skipping versions ('v3 → v5') makes the chain ambiguous to
    /// reason about and forecloses inserting a v3→v4 step later
    /// without invalidating the v3→v5 fixture.
    #[test]
    fn each_migration_advances_by_one() {
        for m in migrations() {
            assert_eq!(
                m.to_version(),
                m.from_version() + 1,
                "{} jumps from v{} to v{} — migrations must advance by exactly 1",
                std::any::type_name_of_val(&*m),
                m.from_version(),
                m.to_version(),
            );
        }
    }

    #[test]
    fn current_version_passes_through_unchanged() {
        let payload = json!({
            "schema_version": ONTOLOGY_IR_SCHEMA_VERSION,
            "id": "ont",
            "name": "Test",
        });
        let out = migrate_to_current(payload.clone()).unwrap();
        assert_eq!(out, payload);
    }

    #[test]
    fn missing_schema_version_treated_as_current() {
        // Same rationale as the deserialiser: `serde(default = ...)`
        // gives the absent tag the current value. The migrator must
        // mirror that.
        let payload = json!({"id": "ont", "name": "Test"});
        let out = migrate_to_current(payload.clone()).unwrap();
        // No transformation applied (already at current).
        assert_eq!(out, payload);
    }

    #[test]
    fn future_version_is_rejected() {
        let payload = json!({
            "schema_version": ONTOLOGY_IR_SCHEMA_VERSION + 1,
            "id": "ont",
        });
        let err = migrate_to_current(payload).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("newer"),
            "rejection should name the version skew: {msg}",
        );
    }
}
