//! Unified ontology-creation endpoint — `POST /api/ontology`.
//!
//! Creates an empty ontology and applies an optional batch of
//! [`OntologyEditOp`]s as the first committed version. Reuses the
//! same Phase-6 approval routing + validation pipeline as
//! `POST /api/ontology/edits`, so *every* ontology content
//! change — creation or subsequent edit — flows through one
//! auditable machinery.
//!
//! The older `/api/bootstrap/seed-glossary` endpoint was the only
//! creation path before this module existed; it has been removed in
//! favour of this single canonical entry point.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use ox_core::i18n::LocalizedText;
use ox_ontology::{OntologyEditOp, OntologyIR};

use crate::error::AppError;
use crate::principal::Principal;
use crate::response::ApiResponse;
use crate::routes::ontology::routing::verify_ops_apply;

/// Upper bound for the trimmed ontology name. The DB column is
/// `TEXT` (no hard limit) — the ceiling is a policy choice to keep
/// the Map / list UIs rendering predictably and to defeat accidental
/// paste-bomb inputs.
const MAX_NAME_LEN: usize = 256;
use crate::state::AppState;

/// Request body for `POST /api/ontology`.
///
/// `initial_operations` is optional — callers that only need the
/// shell (empty ontology with a pilot name) can omit it. When
/// present, the ops apply atomically: an error on any op or on the
/// post-batch `validate()` aborts the whole creation with nothing
/// persisted.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct CreateOntologyRequest {
    /// Workspace-scoped name. Must be non-empty and unique within
    /// the workspace (enforced by the store's `ontologies_ws_name_uq`
    /// constraint).
    pub name: String,
    /// Free-form description stored as the ontology's default
    /// `LocalizedText`. Whitespace-only input collapses to the empty
    /// localized text.
    #[serde(default)]
    pub description: Option<String>,
    /// Optional caller-supplied lineage id. When omitted a fresh
    /// UUID seeds the lineage so cross-version references remain
    /// stable across later refinements.
    #[serde(default)]
    pub lineage_id: Option<String>,
    /// Initial batch of edit operations applied atomically as v1.
    /// Must contain at least one op — the handler rejects empty
    /// batches with 400 so the endpoint stays symmetric with
    /// `/edits`, which never accepts an empty operations list.
    #[serde(default)]
    pub initial_operations: Vec<OntologyEditOp>,
    /// Free-form commit message — surfaces in the version log next
    /// to the first snapshot.
    #[serde(default)]
    pub message: Option<String>,
}

/// Response body on successful creation. Carries enough data for
/// the FE to deep-link to the new ontology without a round-trip
/// refetch.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct CreateOntologyResponse {
    pub ontology_id: Uuid,
    pub version_id: Uuid,
    pub version: u32,
    pub applied_operations: usize,
    pub committed_at: DateTime<Utc>,
}

#[utoipa::path(
    post,
    path = "/api/ontology",
    request_body = CreateOntologyRequest,
    responses(
        (status = 201, description = "Ontology created — returns identity + first version"),
        (status = 400, description = "Missing name or other client-side input error"),
        (status = 409, description = "Workspace already has an ontology, or initial operation would queue for approval"),
        (status = 422, description = "Initial operation or post-batch validation rejected the IR"),
    ),
    security(("api_key" = [])),
    tag = "Ontology",
)]
pub(crate) async fn create_ontology(
    State(state): State<AppState>,
    principal: Principal,
    Json(req): Json<CreateOntologyRequest>,
) -> Result<(StatusCode, Json<ApiResponse<CreateOntologyResponse>>), AppError> {
    principal.require_designer()?;

    let name = req.name.trim();
    if name.is_empty() {
        return Err(AppError::required_field_empty("name"));
    }
    if name.chars().count() > MAX_NAME_LEN {
        return Err(AppError::text_length_out_of_range("name", 1, MAX_NAME_LEN));
    }
    if name.chars().any(char::is_control) {
        return Err(AppError::identifier_format_invalid(
            "name",
            name.to_string(),
            "printable_text",
        ));
    }
    // Reject the degenerate shape. `/edits` enforces the same invariant
    // (see `edits.rs`); keeping both endpoints symmetric means callers
    // never have to guess which one accepts an empty batch.
    if req.initial_operations.is_empty() {
        return Err(AppError::required_field_empty("initial_operations"));
    }

    // ---- 1. Routing ---------------------------------------------
    //
    // Routing is pure (classify + role + code-count delta) so it
    // runs first — a queue decision here means we never touch the
    // store. See `routing.rs` for the full pipeline ordering
    // rationale.
    verify_ops_apply(&state, &principal, &req.initial_operations).await?;

    let description_lt = match req.description.as_deref().map(str::trim) {
        Some(s) if !s.is_empty() => LocalizedText::from(s),
        _ => LocalizedText::default(),
    };

    let lineage_seed = req
        .lineage_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    // ---- 2. Apply ops to an empty IR ----------------------------
    let mut ir = OntologyIR::new(
        lineage_seed.clone(),
        name.to_string(),
        description_lt.clone(),
        1u32,
        Vec::new(),
        Vec::new(),
        Vec::new(),
    );

    // The per-op error surfaces the op index so the FE can
    // highlight which row failed in a multi-op wizard.
    for (idx, op) in req.initial_operations.iter().enumerate() {
        op.apply_to(&mut ir).map_err(|e| {
            AppError::edit_operation_rejected(format!("initial_operations[{idx}]: {e}"))
        })?;
    }

    // ---- 3. Whole-IR validation ---------------------------------
    //
    // Catches referential-integrity violations across the batch
    // (e.g. a `BindPropertyToTerm` referencing a term that sibling
    // ops didn't declare). Must pass before any persistence side
    // effect fires.
    let validation = ir.validate();
    if !validation.is_empty() {
        return Err(AppError::ontology_invariant_violation(validation));
    }

    // ---- 4. Commit ---------------------------------------------

    let description_json = serde_json::to_value(&description_lt)
        .map_err(|e| AppError::internal(format!("serialize description: {e}")))?;
    let display_name_json = serde_json::to_value(&ir.display_name)
        .map_err(|e| AppError::internal(format!("serialize display_name: {e}")))?;

    let identity = state
        .store
        .create_ontology(
            name,
            &display_name_json,
            &description_json,
            Some(&lineage_seed),
        )
        .await
        .map_err(AppError::from)?;

    let committed_by = principal
        .user_uuid()
        .map(|u| u.to_string())
        .unwrap_or_else(|_| "apikey".into());
    let commit_message = req
        .message
        .as_deref()
        .unwrap_or("ontology created via admin API");

    let capture = ox_ontology::ProvenanceCapture::ontology_edit(
        ox_ontology::AgentRef::User {
            user_id: committed_by.clone(),
        },
        commit_message,
    );

    let snapshot = state
        .store
        .commit_version(
            identity.id,
            &ir,
            "1",
            None,
            &committed_by,
            commit_message,
            capture,
            ox_text::glossary_tokenizer_fingerprint(&ir).as_str(),
        )
        .await
        .map_err(AppError::from)?;

    // Build the workspace tokenizer + backfill any retrieval
    // surfaces seeded by the bootstrap. First-version commit
    // → no parent → publish-on-mismatch always fires when the
    // ontology already carries glossary terms.
    crate::tokenizer_publish::publish_workspace_tokenizer_after_commit(
        &state,
        identity.workspace_id,
        None,
        &ir,
    )
    .await
    .map_err(AppError::from)?;

    Ok((
        StatusCode::CREATED,
        ApiResponse::of(CreateOntologyResponse {
            ontology_id: identity.id,
            version_id: snapshot.id,
            version: 1,
            applied_operations: req.initial_operations.len(),
            committed_at: snapshot.created_at,
        }),
    ))
}
