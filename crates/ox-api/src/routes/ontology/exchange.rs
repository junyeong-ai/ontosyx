use axum::Json;

use ox_compiler::export;
use ox_compiler::import;
use ox_ontology::input::{InputOntologyDef, normalize, to_exchange_format};
use ox_ontology::ir::OntologyIR;
use ox_ontology::mapping::SourceMapping;
use serde::Deserialize;

use crate::error::AppError;
use crate::response::ApiResponse;

// ---------------------------------------------------------------------------
// Normalize — InputOntologyDef → OntologyIR
// ---------------------------------------------------------------------------

#[utoipa::path(
    post,
    path = "/api/ontology/normalize",
    request_body(content = Object, description = "InputOntologyDef to normalize"),
    responses(
        (status = 200, description = "Normalized OntologyIR", body = Object),
        (status = 400, description = "Validation errors", body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Ontologies",
)]
pub(crate) async fn normalize_ontology(
    Json(input): Json<InputOntologyDef>,
) -> Result<Json<ApiResponse<serde_json::Value>>, AppError> {
    let result = normalize(input).map_err(|errors| AppError::bad_request(errors.join("; ")))?;
    Ok(ApiResponse::of(serde_json::json!({
        "ontology": result.ontology,
        "warnings": result.warnings,
    })))
}

// ---------------------------------------------------------------------------
// Export — OntologyIR → InputOntologyDef (exchange format)
// ---------------------------------------------------------------------------

#[utoipa::path(
    post,
    path = "/api/ontology/export",
    request_body(content = Object, description = "OntologyIR to export"),
    responses(
        (status = 200, description = "InputOntologyDef exchange format", body = Object),
    ),
    security(("api_key" = [])),
    tag = "Ontologies",
)]
pub(crate) async fn export_ontology(
    Json(ontology): Json<OntologyIR>,
) -> Result<Json<ApiResponse<InputOntologyDef>>, AppError> {
    let exchange = to_exchange_format(&ontology, &SourceMapping::new());
    Ok(ApiResponse::of(exchange))
}

// ---------------------------------------------------------------------------
// Plain-text exporters: each emits a different target language/format
// ---------------------------------------------------------------------------

#[utoipa::path(
    post,
    path = "/api/ontology/export/cypher",
    request_body(content = Object, description = "OntologyIR"),
    responses(
        (status = 200, description = "Cypher DDL statements", content_type = "text/plain"),
    ),
    security(("api_key" = [])),
    tag = "Ontologies",
)]
pub(crate) async fn export_cypher(Json(ontology): Json<OntologyIR>) -> Result<String, AppError> {
    Ok(export::generate_cypher_ddl(&ontology))
}

#[utoipa::path(
    post,
    path = "/api/ontology/export/mermaid",
    request_body(content = Object, description = "OntologyIR"),
    responses(
        (status = 200, description = "Mermaid ER diagram", content_type = "text/plain"),
    ),
    security(("api_key" = [])),
    tag = "Ontologies",
)]
pub(crate) async fn export_mermaid(Json(ontology): Json<OntologyIR>) -> Result<String, AppError> {
    Ok(export::generate_mermaid(&ontology))
}

#[utoipa::path(
    post,
    path = "/api/ontology/export/graphql",
    request_body(content = Object, description = "OntologyIR"),
    responses(
        (status = 200, description = "GraphQL schema", content_type = "text/plain"),
    ),
    security(("api_key" = [])),
    tag = "Ontologies",
)]
pub(crate) async fn export_graphql(Json(ontology): Json<OntologyIR>) -> Result<String, AppError> {
    Ok(export::generate_graphql(&ontology))
}

#[utoipa::path(
    post,
    path = "/api/ontology/export/owl",
    request_body(content = Object, description = "OntologyIR"),
    responses(
        (status = 200, description = "OWL/Turtle ontology", content_type = "text/plain"),
    ),
    security(("api_key" = [])),
    tag = "Ontologies",
)]
pub(crate) async fn export_owl(Json(ontology): Json<OntologyIR>) -> Result<String, AppError> {
    Ok(export::generate_owl_turtle(&ontology))
}

#[utoipa::path(
    post,
    path = "/api/ontology/export/shacl",
    request_body(content = Object, description = "OntologyIR"),
    responses(
        (status = 200, description = "SHACL shapes in Turtle format", content_type = "text/plain"),
    ),
    security(("api_key" = [])),
    tag = "Ontologies",
)]
pub(crate) async fn export_shacl(Json(ontology): Json<OntologyIR>) -> Result<String, AppError> {
    Ok(export::generate_shacl(&ontology))
}

#[utoipa::path(
    post,
    path = "/api/ontology/export/typescript",
    request_body(content = Object, description = "OntologyIR"),
    responses(
        (status = 200, description = "TypeScript interface definitions", content_type = "text/plain"),
    ),
    security(("api_key" = [])),
    tag = "Ontologies",
)]
pub(crate) async fn export_typescript(
    Json(ontology): Json<OntologyIR>,
) -> Result<String, AppError> {
    Ok(export::generate_typescript(&ontology))
}

#[utoipa::path(
    post,
    path = "/api/ontology/export/python",
    request_body(content = Object, description = "OntologyIR"),
    responses(
        (status = 200, description = "Python dataclass definitions", content_type = "text/plain"),
    ),
    security(("api_key" = [])),
    tag = "Ontologies",
)]
pub(crate) async fn export_python(Json(ontology): Json<OntologyIR>) -> Result<String, AppError> {
    Ok(export::generate_python(&ontology))
}

// ---------------------------------------------------------------------------
// Import — OWL/Turtle → OntologyIR
// ---------------------------------------------------------------------------

#[derive(Deserialize, utoipa::ToSchema)]
pub struct OntologyImportRequest {
    /// OWL ontology in Turtle format.
    pub content: String,
}

#[utoipa::path(
    post,
    path = "/api/ontology/import/owl",
    request_body = OntologyImportRequest,
    responses(
        (status = 200, description = "Parsed OntologyIR", body = Object),
        (status = 400, description = "Parse or validation errors", body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("api_key" = [])),
    tag = "Ontologies",
)]
pub(crate) async fn import_owl(
    Json(req): Json<OntologyImportRequest>,
) -> Result<Json<ApiResponse<OntologyIR>>, AppError> {
    if req.content.trim().is_empty() {
        return Err(AppError::bad_request("content must not be empty"));
    }
    let ontology =
        import::parse_owl_turtle(&req.content).map_err(|e| AppError::bad_request(e.to_string()))?;
    Ok(ApiResponse::of(ontology))
}
