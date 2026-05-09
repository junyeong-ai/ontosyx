//! Single writer for [`SourceMappingArtifact`] persistence at the
//! tail of every LLM-driven design action.
//!
//! Route handlers run the LLM, get back the produced
//! [`OntologyIR`] paired with the call's
//! [`ArtifactProvenance`], and call [`persist_design_artifact`] to
//! durably record the source-to-IR mapping. The artifact store is
//! content-addressed — a re-run of the same design call against an
//! unchanged schema collapses to a single row instead of duplicating.

use ox_core::source_schema::SourceSchema;
use ox_ontology::ir::OntologyIR;
use ox_ontology::mapping::SourceId;
use ox_ontology::source_mapping::{ArtifactProvenance, SourceMappingArtifact};
use ox_store::SourceMappingArtifactStore;
use tracing::{info, warn};

/// Author and persist a [`SourceMappingArtifact`] for a freshly-
/// designed ontology. Idempotent on a repeat call with identical
/// schema + body — the store layer's
/// `(workspace_id, source_id, schema_snapshot_hash, content_hash)`
/// unique constraint absorbs duplicates.
///
/// Failures are logged but do not propagate. A transient store error
/// must not block the design flow from succeeding — the ontology has
/// already been persisted by the caller and the artifact replays
/// from the same inputs the next time the action runs.
///
/// Bounded on `SourceMappingArtifactStore` rather than the full
/// `Store` supertrait so the helper exposes only the surface it
/// actually consumes — easier to mock in unit tests, narrower
/// dependency on the store layer.
pub(crate) async fn persist_design_artifact<S>(
    store: &S,
    ontology: &OntologyIR,
    source_id: &SourceId,
    schema: &SourceSchema,
    provenance: ArtifactProvenance,
    created_by: impl Into<String>,
) where
    S: SourceMappingArtifactStore + ?Sized,
{
    let artifact = SourceMappingArtifact::derive_from_design(
        ontology, source_id, schema, provenance, created_by,
    );
    let artifact_id = artifact.id.clone();
    let schema_hash = artifact.schema_snapshot_hash.clone();
    match store.create_artifact(artifact).await {
        Ok(persisted) => {
            info!(
                artifact_id = %persisted.id,
                source_id = %persisted.source_id,
                schema_hash = %persisted.schema_snapshot_hash,
                property_mappings = persisted.property_mappings.len(),
                edge_mappings = persisted.edge_mappings.len(),
                "Persisted SourceMappingArtifact"
            );
        }
        Err(error) => {
            warn!(
                ?error,
                artifact_id = %artifact_id,
                source_id = %source_id,
                schema_hash = %schema_hash,
                "Failed to persist SourceMappingArtifact — design action proceeds, \
                 artifact will replay on the next run"
            );
        }
    }
}
