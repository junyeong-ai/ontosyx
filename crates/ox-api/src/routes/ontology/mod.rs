//! Ontology HTTP routes.
//!
//! Module layout:
//! - [`crud`]          — list, apply commands
//! - [`exchange`]      — normalize, export to multiple formats, OWL import
//! - [`verifications`] — element verification CRUD
//! - [`schema_ops`]    — reindex, audit vs live graph, adopt graph schema,
//!                       LLM-backed suggestions + data-sample enrichment

mod crud;
mod exchange;
mod schema_ops;
mod verifications;

// Handler functions are referenced by the router (`routes::mod::router`) via
// these module-local names, so `pub(crate) use` keeps the surface minimal.
pub(crate) use crud::{apply_ontology_commands, list_ontologies};
pub(crate) use exchange::{
    export_cypher, export_graphql, export_mermaid, export_ontology, export_owl, export_python,
    export_shacl, export_typescript, import_owl, normalize_ontology,
};
pub(crate) use schema_ops::{
    adopt_graph, enrich_ontology, graph_audit_report, reindex_schema, suggest_insights,
};
pub(crate) use verifications::{delete_verification, list_verifications, verify_element};

// `#[utoipa::path]` generates hidden `__path_<fn>` structs in the defining
// module. The `OpenApi` derive resolves them via the path written in its
// `paths(...)` list (e.g. `ontology::list_ontologies`), so we must re-export
// the path symbols from here as well. `pub use` is required because the
// generated structs are `pub`; `pub(crate) use` would narrow them and break
// the derive's resolution.
pub use crud::{__path_apply_ontology_commands, __path_list_ontologies};
pub use exchange::{
    __path_export_cypher, __path_export_graphql, __path_export_mermaid, __path_export_ontology,
    __path_export_owl, __path_export_python, __path_export_shacl, __path_export_typescript,
    __path_import_owl, __path_normalize_ontology,
};
// Request/response types referenced by `openapi::ApiDoc::components(schemas(...))`
// — utoipa needs them visible at this canonical module path.
pub use crud::{OntologyCommandsRequest, OntologyCommandsResponse};
pub use exchange::OntologyImportRequest;
