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
mod routing;
mod schema_ops;
mod type_candidates;
mod validate;
mod value_sets;
mod verifications;

// Handler functions are referenced by the router (`routes::mod::router`) via
// these module-local names, so `pub(crate) use` keeps the surface minimal.
pub(crate) use crud::{apply_ontology_commands, get_ontology_detail, list_ontologies};
pub(crate) use axis_items::list_axis_items;
pub(crate) use create::create_ontology;
pub(crate) use cross_refs::list_cross_refs;
pub(crate) use dependencies::get_ontology_dependencies;
pub(crate) use validate::get_ontology_validate;
pub(crate) use map_summary::map_summary;
pub(crate) use edits::apply_ontology_edits;
pub(crate) use type_candidates::list_type_candidates;
pub(crate) use exchange::{
    export_cypher, export_graphql, export_mermaid, export_ontology, export_owl, export_python,
    export_shacl, export_typescript, import_owl, normalize_ontology,
};
pub(crate) use schema_ops::{
    adopt_graph, enrich_ontology, graph_audit_report, reindex_schema, suggest_insights,
};
pub(crate) use binding_suggestions::{
    suggest_glossary_bindings, suggest_glossary_terms_for_property,
};
pub(crate) use value_sets::propose_ontology_value_sets;
pub(crate) use verifications::{delete_verification, list_verifications, verify_element};

// `#[utoipa::path]` generates hidden `__path_<fn>` structs in the defining
// module. The `OpenApi` derive resolves them via the path written in its
// `paths(...)` list (e.g. `ontology::list_ontologies`), so we must re-export
// the path symbols from here as well. `pub use` is required because the
// generated structs are `pub`; `pub(crate) use` would narrow them and break
// the derive's resolution.
pub use crud::{
    __path_apply_ontology_commands, __path_get_ontology_detail, __path_list_ontologies,
};
pub use create::__path_create_ontology;
pub use dependencies::__path_get_ontology_dependencies;
pub use validate::__path_get_ontology_validate;
pub use edits::__path_apply_ontology_edits;
pub use value_sets::__path_propose_ontology_value_sets;
pub use binding_suggestions::{
    __path_suggest_glossary_bindings, __path_suggest_glossary_terms_for_property,
};
pub use exchange::{
    __path_export_cypher, __path_export_graphql, __path_export_mermaid, __path_export_ontology,
    __path_export_owl, __path_export_python, __path_export_shacl, __path_export_typescript,
    __path_import_owl, __path_normalize_ontology,
};
// Request/response types referenced by `openapi::ApiDoc::components(schemas(...))`
// — utoipa needs them visible at this canonical module path.
pub use create::{CreateOntologyRequest, CreateOntologyResponse};
pub use crud::{
    CurrentVersionSummary, ApplyOntologyCommandsRequest, ApplyOntologyCommandsResponse, OntologyDetail,
    OntologyListItem,
};
pub use exchange::ImportOntologyRequest;
pub use value_sets::{
    EvidenceBody, ProposalBody, ProposePolicyBody, ProposeValueSetsRequest,
    ProposeValueSetsResponse, SkipBody,
};
pub use binding_suggestions::{
    BindingPolicyBody, PropertyCandidateBody, SignalBody, SuggestBindingsRequest,
    SuggestBindingsResponse, SuggestTermsRequest, SuggestTermsResponse, TermCandidateBody,
};
