//! Ontology HTTP routes.
//!
//! Module layout:
//! - [`crud`]          — list, apply commands
//! - [`exchange`]      — normalize, export to multiple formats, OWL import
//! - [`verifications`] — element verification CRUD
//! - [`schema_ops`]    — reindex, audit vs live graph, adopt graph schema,
//!   LLM-backed suggestions + data-sample enrichment

mod binding_suggestions;
mod crud;
mod edits;
mod exchange;
mod map_summary;
mod schema_ops;
mod type_candidates;
mod value_sets;
mod verifications;

// Handler functions are referenced by the router (`routes::mod::router`) via
// these module-local names, so `pub(crate) use` keeps the surface minimal.
pub(crate) use crud::{apply_ontology_commands, get_ontology_detail, list_ontologies};
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
pub use crud::{
    CurrentVersionSummary, OntologyCommandsRequest, OntologyCommandsResponse, OntologyDetail,
    OntologyListItem,
};
pub use exchange::OntologyImportRequest;
pub use value_sets::{
    EvidenceBody, ProposalBody, ProposePolicyBody, ProposeValueSetsRequest,
    ProposeValueSetsResponse, SkipBody,
};
pub use binding_suggestions::{
    BindingPolicyBody, PropertyCandidateBody, SignalBody, SuggestBindingsRequest,
    SuggestBindingsResponse, SuggestTermsRequest, SuggestTermsResponse, TermCandidateBody,
};
