//! [`DesignOntologyInput`] — structured input for
//! `OntologyDesigner::design_ontology`.
//!
//! Replaces the previous `(sample_data, context, source_id)` triple
//! with a struct carrying the **domain context** (glossary, code
//! systems, ambiguity hints, existing ontology) the workspace
//! already knows about. The LLM consumes that context up-front so it
//! prefers existing canonical terms over inventing parallel labels
//! for the same concept — making the post-pass `binding_suggestions`
//! a fall-back rather than the primary correction step.
//!
//! Every domain slice is `&[T]` so passing an empty slice keeps the
//! call shape identical to "no context" — empty inputs render as
//! empty prompt sections, the LLM behaves as it always did.

use ox_ontology::OntologyIR;
use ox_ontology::ambiguity::AmbiguityContext;
use ox_ontology::code_system::CodeSystemDef;
use ox_ontology::glossary::GlossaryTermDef;
use ox_ontology::mapping::SourceId;

/// Structured input the LLM consumes when designing or extending an
/// ontology. Borrows everything — the caller assembles slices from
/// owned domain artefacts (`OntologyIR::glossary()`,
/// `OntologyIR::code_systems()`, etc.) without a clone.
#[derive(Debug, Clone, Copy)]
pub struct DesignOntologyInput<'a> {
    /// Pre-formatted source data text — sample CSV / JSON, formatted
    /// DB schema + statistics, or the analyser's compressed
    /// `SourceAnalysisReport`. The orchestration layer (today
    /// `crates/ox-api/src/routes/projects/helpers/llm.rs`) is in
    /// charge of selecting and serialising this.
    pub sample_data: &'a str,

    /// Pre-formatted free-form context — table clusters, design
    /// options, repository field hints. Same orchestration source as
    /// `sample_data`.
    pub context: &'a str,

    /// Canonical source identity stamped onto every emitted
    /// `ObjectMappingDef` in the returned IR. Keeps federation
    /// plans, provenance, and plan-cache keys consistent with the
    /// `OntologyDraft` the caller is operating on.
    pub source_id: &'a SourceId,

    /// Domain glossary the LLM should reference instead of inventing
    /// new node / edge labels for already-defined business concepts.
    /// Empty when the workspace has no glossary yet — the LLM
    /// behaves as it always did.
    pub glossary_terms: &'a [GlossaryTermDef],

    /// Already-registered code systems (national codes, units,
    /// internal taxonomies). The LLM should reference these by id in
    /// property descriptions instead of redefining their codes
    /// inline.
    pub code_systems: &'a [CodeSystemDef],

    /// Ambiguities the introspection / planner pipeline detected but
    /// has not yet resolved. Surfaced to the LLM so it can either
    /// pick a canonical interpretation or recommend a code system /
    /// glossary term in the property description.
    pub ambiguity_hints: &'a [AmbiguityContext],

    /// When extending an existing ontology, the live IR. The LLM
    /// should produce only the *new* node / edge / property
    /// definitions not already covered, leaving existing labels
    /// intact for the merge pass to absorb.
    pub existing_ontology: Option<&'a OntologyIR>,
}

impl<'a> DesignOntologyInput<'a> {
    /// Convenience constructor for the "no domain context yet" case
    /// — every workspace starts here. Equivalent to building the
    /// struct with empty domain slices.
    pub fn bare(sample_data: &'a str, context: &'a str, source_id: &'a SourceId) -> Self {
        Self {
            sample_data,
            context,
            source_id,
            glossary_terms: &[],
            code_systems: &[],
            ambiguity_hints: &[],
            existing_ontology: None,
        }
    }

    /// Whether any domain-context slot is populated. Useful for
    /// metrics ("how often is the LLM running with vs without
    /// context?") and for log lines that summarize a design call.
    pub fn has_domain_context(&self) -> bool {
        !self.glossary_terms.is_empty()
            || !self.code_systems.is_empty()
            || !self.ambiguity_hints.is_empty()
            || self.existing_ontology.is_some()
    }
}
