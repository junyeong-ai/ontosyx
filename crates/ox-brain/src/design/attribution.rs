//! [`DesignAttribution`] + [`DesignOntologyOutput`] — provenance the
//! caller needs to author a `SourceMappingArtifact` against the
//! design action's output.
//!
//! Brain methods that drive the LLM design pipeline return both the
//! `OntologyIR` and the attribution that authored it (prompt id +
//! version, model id, optional knob params). The API layer threads
//! attribution into the `ArtifactProvenance` envelope on the
//! resulting [`ox_ontology::source_mapping::SourceMappingArtifact`]
//! so a later viewer can answer "who/what produced this mapping".

use std::collections::BTreeMap;

use ox_ontology::ir::OntologyIR;
use serde::{Deserialize, Serialize};

/// Provenance envelope describing the LLM call that produced an
/// ontology design output. Mirrors
/// [`ox_ontology::source_mapping::ArtifactProvenance`] — the API
/// layer typically converts directly between the two.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DesignAttribution {
    /// `prompt_templates.name` of the prompt used (e.g.
    /// `design_ontology`, `design_ontology_batch`,
    /// `refine_ontology`).
    pub prompt_id: String,
    /// Semver-ish version string of the prompt at call time. Captured
    /// from the registry's loaded version, so an admin updating the
    /// prompt mid-flight does not retroactively rewrite older
    /// artifacts.
    pub prompt_version: String,
    /// Resolved model id (`anthropic:claude-sonnet-4-6`,
    /// `openai:gpt-5`, …). The model resolver picks this from
    /// per-operation routing rules at the moment the call ran.
    pub model_id: String,
    /// Free-form, opaque parameters useful for replay / debugging —
    /// temperature, seed, knobs the prompt uses. `BTreeMap` for
    /// stable serialisation order.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub params: BTreeMap<String, serde_json::Value>,
}

impl DesignAttribution {
    /// Construct an attribution with no extra params.
    pub fn new(
        prompt_id: impl Into<String>,
        prompt_version: impl Into<String>,
        model_id: impl Into<String>,
    ) -> Self {
        Self {
            prompt_id: prompt_id.into(),
            prompt_version: prompt_version.into(),
            model_id: model_id.into(),
            params: BTreeMap::new(),
        }
    }
}

/// Output of an LLM-driven ontology design call. Bundles the
/// produced [`OntologyIR`] with the [`DesignAttribution`] that
/// authored it so the caller can author a `SourceMappingArtifact`
/// atomically — same call, same provenance.
#[derive(Debug, Clone)]
pub struct DesignOntologyOutput {
    pub ontology: OntologyIR,
    pub attribution: DesignAttribution,
}
