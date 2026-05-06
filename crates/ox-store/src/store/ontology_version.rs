//! Versioned ontology storage backed by content-addressed entity
//! extraction.
//!
//! Commits extract an `OntologyIR` into content-addressed entities
//! (via `ox_ontology::storage::extract_entities`), INSERT ON CONFLICT
//! DO NOTHING into the Level 2 store, and write a new pointer set.
//! Loads rehydrate a version by joining the pointer set with the
//! entity store.

use async_trait::async_trait;
use uuid::Uuid;

use ox_core::error::OxResult;

use crate::models::{EntityChange, OntologyRow, OntologyVersionSnapshot};

#[async_trait]
pub trait OntologyVersionStore: Send + Sync {
    /// Create the workspace's canonical ontology. The workspace ×
    /// ontology cardinality is 1:1; calling this twice in the same
    /// workspace fails the UNIQUE constraint. Assigns a fresh
    /// lineage_id if `lineage_id` is `None`.
    async fn create_ontology(
        &self,
        name: &str,
        display_name: &serde_json::Value,
        description: &serde_json::Value,
        lineage_id: Option<&str>,
    ) -> OxResult<OntologyRow>;

    /// Singleton accessor — return the workspace's canonical
    /// ontology, or `None` when one has not been created yet.
    /// Workspace × ontology is 1:1 by schema invariant; the
    /// workspace_id is the implicit selector via the task-local
    /// context. This is the single read path; there is no
    /// `get_ontology(id)`, no list, no name lookup.
    async fn get_workspace_ontology(&self) -> OxResult<Option<OntologyRow>>;

    /// Commit a new immutable version of `ontology_id`.
    ///
    /// Pipeline:
    /// 1. Extract entities from `ir` via
    ///    `ox_ontology::storage::extract_entities`.
    /// 2. `INSERT ... ON CONFLICT (entity_hash) DO NOTHING`
    ///    into `ontology_entity_versions` — automatic dedup of
    ///    unchanged entities across versions.
    /// 3. Insert a fresh `ontology_version_snapshots` row with
    ///    the new version tag + bitemporal columns.
    /// 4. Bulk-insert the pointer set into
    ///    `ontology_version_entities`.
    ///
    /// Executes in a single transaction — either the whole
    /// commit lands or none of it does.
    async fn commit_version(
        &self,
        ontology_id: Uuid,
        ir: &ox_ontology::OntologyIR,
        version: &str,
        parent_version_id: Option<Uuid>,
        committed_by: &str,
        commit_message: &str,
    ) -> OxResult<OntologyVersionSnapshot>;

    /// Hydrate the ontology at a given version. Joins pointer set
    /// with entity store, rehydrates each entity's `content`
    /// JSONB into the typed `XxxDef`, and assembles the full
    /// `OntologyIR`.
    ///
    /// Returns `Ok(None)` when no snapshot exists for `version_id`
    /// (e.g., the snapshot was deleted between a prior lookup and
    /// this hydrate, or the caller was handed a stale handle).
    /// Returns `Err` only when stored entities are malformed
    /// (parse / deserialization failure or missing header).
    async fn get_ontology_ir(
        &self,
        version_id: Uuid,
    ) -> OxResult<Option<ox_ontology::OntologyIR>>;

    /// Fetch a version snapshot record by id (without hydrating
    /// the full IR). Used by routes that need version metadata
    /// (committed_by, commit_message, valid_from) separate from
    /// the IR content.
    async fn get_version_snapshot(
        &self,
        version_id: Uuid,
    ) -> OxResult<Option<OntologyVersionSnapshot>>;

    /// List the version history of an ontology, newest first.
    async fn list_versions(
        &self,
        ontology_id: Uuid,
        limit: u32,
    ) -> OxResult<Vec<OntologyVersionSnapshot>>;

    /// "Live at" version resolver. Picks the newest version
    /// whose `valid_from <= as_of` AND (`valid_to IS NULL OR
    /// valid_to > as_of`). Used by TemporalRewriter for AS-OF
    /// queries.
    async fn resolve_version_at(
        &self,
        ontology_id: Uuid,
        as_of: chrono::DateTime<chrono::Utc>,
    ) -> OxResult<Option<OntologyVersionSnapshot>>;

    /// The current (valid_to IS NULL) version for an ontology.
    async fn find_current_version(
        &self,
        ontology_id: Uuid,
    ) -> OxResult<Option<OntologyVersionSnapshot>>;

    /// Diff two versions. Returns one `EntityChange` per
    /// `(kind, logical_id)` whose hash differs. Order: kind then
    /// logical_id — stable for UI rendering.
    async fn diff_versions(
        &self,
        from_version: Uuid,
        to_version: Uuid,
    ) -> OxResult<Vec<EntityChange>>;
}
