//! Helpers that translate brain-side
//! [`DesignAttribution`](ox_brain::DesignAttribution) into a
//! persisted [`SourceMappingArtifact`](ox_ontology::source_mapping::SourceMappingArtifact).
//!
//! Every LLM-driven design action ends here: the route handler runs
//! the LLM, takes the resulting `(ontology, attribution)` pair, and
//! calls [`persist_design_artifact`] to durably record the
//! source-to-IR mapping that the call produced. The artifact store is
//! content-addressed — a re-run of the same design call against an
//! unchanged schema collapses to a single row instead of duplicating.

use ox_brain::DesignAttribution;
use ox_core::source_schema::SourceSchema;
use ox_ontology::ir::OntologyIR;
use ox_ontology::mapping::SourceId;
use ox_ontology::source_mapping::{ArtifactProvenance, SourceMappingArtifact};
use ox_store::SourceMappingArtifactStore;
use tracing::{info, warn};

/// Translate a brain-side attribution into the artifact-side
/// provenance envelope. Same fields, different home crate — kept as
/// distinct types so `ox-ontology` does not depend on `ox-brain`.
fn provenance_from_attribution(attribution: DesignAttribution) -> ArtifactProvenance {
    ArtifactProvenance {
        prompt_id: attribution.prompt_id,
        prompt_version: attribution.prompt_version,
        model_id: attribution.model_id,
        params: attribution.params,
    }
}

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
    attribution: DesignAttribution,
    created_by: impl Into<String>,
) where
    S: SourceMappingArtifactStore + ?Sized,
{
    let artifact = SourceMappingArtifact::derive_from_design(
        ontology,
        source_id,
        schema,
        provenance_from_attribution(attribution),
        created_by,
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
