use axum::Json;

use ox_compiler::export;
use ox_compiler::import;
use ox_ontology::input::{InputOntologyDef, NormalizeWarning, normalize, to_exchange_format};
use ox_ontology::ir::OntologyIR;
use ox_ontology::mapping::SourceId;
use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::principal::Principal;
use crate::response::ApiResponse;

// ---------------------------------------------------------------------------
// Authorization note (applies to every handler in this module).
//
// `require_auth` middleware at the route-layer level already
// guarantees these endpoints reject anonymous requests. What it
// does NOT do is distinguish roles. Ontology translation,
// inspection, and import are all editor operations — they accept
// or emit schema content that shapes the rest of the platform's
// behaviour, so gating them to `designer` stops a workspace
// `viewer` from, e.g., uploading an OWL file that re-shapes the
// ontology universe or pulling plain-text Cypher DDL that a
// viewer should not see without an explicit elevation.
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Normalize — InputOntologyDef → OntologyIR
// ---------------------------------------------------------------------------

#[derive(Serialize, utoipa::ToSchema)]
pub struct NormalizeOntologyResponse {
    pub ontology: OntologyIR,
    pub warnings: Vec<NormalizeWarning>,
}

#[utoipa::path(
    post,
    path = "/api/ontology/normalize",
    request_body = InputOntologyDef,
    responses(
        (status = 200, description = "Normalized OntologyIR", body = NormalizeOntologyResponse),
        (status = 400, description = "Validation errors", body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Ontology",
)]
pub(crate) async fn normalize_ontology(
    principal: Principal,
    Json(input): Json<InputOntologyDef>,
) -> Result<Json<ApiResponse<NormalizeOntologyResponse>>, AppError> {
    principal.require_designer()?;
    // Ad-hoc normalize — no project / data source attached. The
    // returned IR is for validation / inspection only and carries
    // no ObjectMappingDef entries unless the caller's InputOntologyDef
    // contains source_table declarations. A static `adhoc` source id
    // keeps any emitted mappings identifiable as having come through
    // this path.
    let source_id = SourceId::new("adhoc:normalize-endpoint");
    let result = normalize(input, &source_id).map_err(AppError::ontology_invariant_violation)?;
    Ok(ApiResponse::of(NormalizeOntologyResponse {
        ontology: result.ontology,
        warnings: result.warnings,
    }))
}

// ---------------------------------------------------------------------------
// Export — OntologyIR → InputOntologyDef (exchange format)
// ---------------------------------------------------------------------------

#[utoipa::path(
    post,
    path = "/api/ontology/export",
    request_body = OntologyIR,
    responses(
        (status = 200, description = "InputOntologyDef exchange format", body = InputOntologyDef),
    ),
    security(("api_key" = [])),
    tag = "Ontology",
)]
pub(crate) async fn export_ontology(
    principal: Principal,
    Json(ontology): Json<OntologyIR>,
) -> Result<Json<ApiResponse<InputOntologyDef>>, AppError> {
    principal.require_designer()?;
    let exchange = to_exchange_format(&ontology);
    Ok(ApiResponse::of(exchange))
}

// ---------------------------------------------------------------------------
// Plain-text exporters: each emits a different target language/format
// ---------------------------------------------------------------------------

#[utoipa::path(
    post,
    path = "/api/ontology/export/cypher",
    request_body = OntologyIR,
    responses(
        (status = 200, description = "Cypher DDL statements", content_type = "text/plain"),
    ),
    security(("api_key" = [])),
    tag = "Ontology",
)]
pub(crate) async fn export_cypher(
    principal: Principal,
    Json(ontology): Json<OntologyIR>,
) -> Result<String, AppError> {
    principal.require_designer()?;
    Ok(export::generate_cypher_ddl(&ontology))
}

#[utoipa::path(
    post,
    path = "/api/ontology/export/mermaid",
    request_body = OntologyIR,
    responses(
        (status = 200, description = "Mermaid ER diagram", content_type = "text/plain"),
    ),
    security(("api_key" = [])),
    tag = "Ontology",
)]
pub(crate) async fn export_mermaid(
    principal: Principal,
    Json(ontology): Json<OntologyIR>,
) -> Result<String, AppError> {
    principal.require_designer()?;
    Ok(export::generate_mermaid(&ontology))
}

#[utoipa::path(
    post,
    path = "/api/ontology/export/graphql",
    request_body = OntologyIR,
    responses(
        (status = 200, description = "GraphQL schema", content_type = "text/plain"),
    ),
    security(("api_key" = [])),
    tag = "Ontology",
)]
pub(crate) async fn export_graphql(
    principal: Principal,
    Json(ontology): Json<OntologyIR>,
) -> Result<String, AppError> {
    principal.require_designer()?;
    Ok(export::generate_graphql(&ontology))
}

#[utoipa::path(
    post,
    path = "/api/ontology/export/owl",
    request_body = OntologyIR,
    responses(
        (status = 200, description = "OWL/Turtle ontology", content_type = "text/plain"),
    ),
    security(("api_key" = [])),
    tag = "Ontology",
)]
pub(crate) async fn export_owl(
    principal: Principal,
    Json(ontology): Json<OntologyIR>,
) -> Result<String, AppError> {
    principal.require_designer()?;
    Ok(export::generate_owl_turtle(&ontology))
}

#[utoipa::path(
    post,
    path = "/api/ontology/export/shacl",
    request_body = OntologyIR,
    responses(
        (status = 200, description = "SHACL shapes in Turtle format", content_type = "text/plain"),
    ),
    security(("api_key" = [])),
    tag = "Ontology",
)]
pub(crate) async fn export_shacl(
    principal: Principal,
    Json(ontology): Json<OntologyIR>,
) -> Result<String, AppError> {
    principal.require_designer()?;
    Ok(export::generate_shacl(&ontology))
}

#[utoipa::path(
    post,
    path = "/api/ontology/export/typescript",
    request_body = OntologyIR,
    responses(
        (status = 200, description = "TypeScript interface definitions", content_type = "text/plain"),
    ),
    security(("api_key" = [])),
    tag = "Ontology",
)]
pub(crate) async fn export_typescript(
    principal: Principal,
    Json(ontology): Json<OntologyIR>,
) -> Result<String, AppError> {
    principal.require_designer()?;
    Ok(export::generate_typescript(&ontology))
}

#[utoipa::path(
    post,
    path = "/api/ontology/export/python",
    request_body = OntologyIR,
    responses(
        (status = 200, description = "Python dataclass definitions", content_type = "text/plain"),
    ),
    security(("api_key" = [])),
    tag = "Ontology",
)]
pub(crate) async fn export_python(
    principal: Principal,
    Json(ontology): Json<OntologyIR>,
) -> Result<String, AppError> {
    principal.require_designer()?;
    Ok(export::generate_python(&ontology))
}

// ---------------------------------------------------------------------------
// Import — OWL/Turtle → OntologyIR
// ---------------------------------------------------------------------------

#[derive(Deserialize, utoipa::ToSchema)]
pub struct ImportOntologyRequest {
    /// OWL ontology in Turtle format.
    pub content: String,
}

#[utoipa::path(
    post,
    path = "/api/ontology/import/owl",
    request_body = ImportOntologyRequest,
    responses(
        (status = 200, description = "Parsed OntologyIR", body = OntologyIR),
        (status = 400, description = "Parse or validation errors", body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Ontology",
)]
pub(crate) async fn import_owl(
    principal: Principal,
    Json(req): Json<ImportOntologyRequest>,
) -> Result<Json<ApiResponse<OntologyIR>>, AppError> {
    principal.require_designer()?;
    if req.content.trim().is_empty() {
        return Err(AppError::required_field_empty("content"));
    }
    // OWL imports carry no data-source binding. Use a static id
    // so any future OWL-to-ObjectMapping extension has a stable
    // identifier to associate emitted mappings with.
    let source_id = SourceId::new("adhoc:owl-import");
    let ontology = import::parse_owl_turtle(&req.content, &source_id)
        .map_err(|e| AppError::owl_parse_failed(e.to_string()))?;
    Ok(ApiResponse::of(ontology))
}
