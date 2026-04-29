//! [`DesignOntologyOutput`] — bundles the LLM-produced
//! [`OntologyIR`] with the [`ArtifactProvenance`] that authored it.
//!
//! Brain methods that drive the LLM design pipeline return both the
//! IR and the provenance envelope (prompt id + version, model id,
//! optional knob params). The API layer threads provenance straight
//! into the persisted
//! [`SourceMappingArtifact`](ox_ontology::source_mapping::SourceMappingArtifact)
//! so a later viewer can answer "who/what produced this mapping?"
//! atomically — no second lookup that could drift behind a
//! mid-flight model-config change.
//!
//! There is intentionally no separate brain-side "attribution" type
//! that mirrors `ArtifactProvenance` field-for-field. Carrying the
//! single canonical type from `ox-ontology` here removes the
//! duplicate struct + the converter helper that translated between
//! them, and adds a future-proofing pressure: any field added to
//! `ArtifactProvenance` for artifact authoring is automatically
//! available to brain callers without parallel maintenance.

use ox_ontology::ir::OntologyIR;
use ox_ontology::source_mapping::ArtifactProvenance;

/// Output of an LLM-driven ontology design call. Bundles the
/// produced [`OntologyIR`] with the [`ArtifactProvenance`] that
/// authored it so the caller can author a `SourceMappingArtifact`
/// atomically — same call, same provenance.
#[derive(Debug, Clone)]
pub struct DesignOntologyOutput {
    pub ontology: OntologyIR,
    pub provenance: ArtifactProvenance,
}
