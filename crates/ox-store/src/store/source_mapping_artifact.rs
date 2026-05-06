//! Source-to-IR mapping artifacts — declarative, content-addressed
//! bridge between a source schema snapshot and the OntologyIR
//! derived from it. Immutable by design: callers
//! `create_artifact` (idempotent on body hash + schema hash),
//! `get_artifact` by id, `list_artifacts_by_source` for review,
//! and `delete_artifact` to retract.

use async_trait::async_trait;

use ox_core::error::OxResult;

#[async_trait]
pub trait SourceMappingArtifactStore: Send + Sync {
    /// Insert an artifact, returning the persisted row. Content-
    /// addressed: a second call with the same `(source_id,
    /// schema_snapshot_hash, content_hash)` triple collapses to
    /// the existing row instead of inserting a duplicate.
    async fn create_artifact(
        &self,
        artifact: ox_ontology::source_mapping::SourceMappingArtifact,
    ) -> OxResult<ox_ontology::source_mapping::SourceMappingArtifact>;

    /// Look up an artifact by id within the active workspace.
    async fn get_artifact(
        &self,
        id: &ox_ontology::source_mapping::SourceMappingArtifactId,
    ) -> OxResult<Option<ox_ontology::source_mapping::SourceMappingArtifact>>;

    /// Every artifact authored against the named source, newest
    /// first. The review surface uses this to diff "what was the
    /// last artifact's mapping decisions?" against the current
    /// LLM proposal.
    async fn list_artifacts_by_source(
        &self,
        source_id: &str,
        limit: i64,
    ) -> OxResult<Vec<ox_ontology::source_mapping::SourceMappingArtifact>>;

    /// Remove an artifact by id. Returns `true` when a row was
    /// deleted, `false` when none matched.
    async fn delete_artifact(
        &self,
        id: &ox_ontology::source_mapping::SourceMappingArtifactId,
    ) -> OxResult<bool>;
}
