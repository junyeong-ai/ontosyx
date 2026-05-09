//! Ontology HTTP routes.
//!
//! Module layout:
//! - [`crud`]          — list, apply commands
//! - [`exchange`]      — normalize, export to multiple formats, OWL import
//! - [`verifications`] — element verification CRUD
//! - [`schema_ops`]    — reindex, audit vs live graph, adopt graph schema,
//!   LLM-backed suggestions + data-sample enrichment

mod axis_items;
mod binding_suggestions;
mod create;
mod cross_refs;
mod crud;
mod dependencies;
mod edits;
mod exchange;
mod map_summary;
mod notation_patterns;
mod routing;
mod schema_ops;
mod type_candidates;
mod validate;
mod value_sets;
mod verifications;

// Handler functions are referenced by the router (`routes::mod::router`) via
// these module-local names, so `pub(crate) use` keeps the surface minimal.
pub(crate) use axis_items::list_axis_items;
pub(crate) use binding_suggestions::{
    suggest_concept_property_bindings, suggest_concepts_for_property,
};
pub(crate) use create::create_ontology;
pub(crate) use cross_refs::list_cross_refs;
pub(crate) use crud::{
    apply_ontology_commands, get_workspace_ontology_detail, list_canonical_versions,
};
pub(crate) use dependencies::get_ontology_dependencies;
pub(crate) use edits::apply_ontology_edits;
pub(crate) use exchange::{
    export_cypher, export_graphql, export_mermaid, export_ontology, export_owl, export_python,
    export_shacl, export_typescript, import_owl, normalize_ontology,
};
pub(crate) use map_summary::map_summary;
pub(crate) use notation_patterns::propose_ontology_notation_patterns;
pub(crate) use schema_ops::{
    adopt_graph, enrich_ontology, graph_audit_report, reindex_schema, suggest_insights,
};
pub(crate) use type_candidates::list_type_candidates;
pub(crate) use validate::get_ontology_validate;
pub(crate) use value_sets::propose_ontology_value_sets;
pub(crate) use verifications::{delete_verification, list_verifications, verify_element};

// `#[utoipa::path]` generates hidden `__path_<fn>` structs in the defining
// module. The `OpenApi` derive resolves them via the path written in its
// `paths(...)` list (e.g. `ontology::list_ontologies`), so we must re-export
// the path symbols from here as well. `pub use` is required because the
// generated structs are `pub`; `pub(crate) use` would narrow them and break
// the derive's resolution.
pub use axis_items::__path_list_axis_items;
pub use binding_suggestions::{
    __path_suggest_concept_property_bindings, __path_suggest_concepts_for_property,
};
pub use create::__path_create_ontology;
pub use cross_refs::__path_list_cross_refs;
pub use crud::{
    __path_apply_ontology_commands, __path_get_workspace_ontology_detail,
    __path_list_canonical_versions,
};
pub use dependencies::__path_get_ontology_dependencies;
pub use edits::__path_apply_ontology_edits;
pub use exchange::{
    __path_export_cypher, __path_export_graphql, __path_export_mermaid, __path_export_ontology,
    __path_export_owl, __path_export_python, __path_export_shacl, __path_export_typescript,
    __path_import_owl, __path_normalize_ontology,
};
pub use map_summary::__path_map_summary;
pub use notation_patterns::__path_propose_ontology_notation_patterns;
pub use schema_ops::{
    __path_adopt_graph, __path_enrich_ontology, __path_graph_audit_report, __path_reindex_schema,
    __path_suggest_insights,
};
pub use validate::__path_get_ontology_validate;
pub use value_sets::__path_propose_ontology_value_sets;
// Request/response types referenced by `openapi::ApiDoc::components(schemas(...))`
// — utoipa needs them visible at this canonical module path.
pub use axis_items::{AxisItem, AxisItemsParams};
pub use binding_suggestions::{
    BindingPolicy, ConceptCandidate, PropertyCandidate, SuggestBindingsRequest,
    SuggestBindingsResponse, SuggestConceptsRequest, SuggestConceptsResponse,
};
pub use create::{CreateOntologyRequest, CreateOntologyResponse};
pub use cross_refs::{Axis, CrossRefEdge};
pub use crud::{
    ApplyOntologyCommandsRequest, ApplyOntologyCommandsResponse, CurrentVersionSummary,
    OntologyDetail, OntologyVersionEntry, OntologyVersionsResponse, WorkspaceOntologyResponse,
};
pub use edits::EditOntologyResponse;
pub use exchange::{ImportOntologyRequest, NormalizeOntologyResponse};
pub use map_summary::{AxisCounts, AxisEntry, DanglerEntry, MapSummaryResponse};
pub use notation_patterns::{
    NotationPolicyBody, NotationProposalBody, NotationSkipBody, ProposeNotationPatternsRequest,
    ProposeNotationPatternsResponse,
};
pub use schema_ops::AdoptGraphRequest;
pub use type_candidates::{__path_list_type_candidates, TypeCandidate, TypeCandidatesParams};
pub use value_sets::{
    EvidenceBody, ProposalBody, ProposePolicyBody, ProposeValueSetsRequest,
    ProposeValueSetsResponse, SkipBody,
};
pub use verifications::{
    __path_delete_verification, __path_list_verifications, __path_verify_element,
    VerifyElementRequest, VerifyElementResponse,
};
