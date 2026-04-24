//! Admin edit-log endpoint — applies a batch of
//! [`ox_ontology::OntologyEditOp`] to a committed ontology version.
//!
//! Flow:
//! 1. Load current version from `OntologyVersionStore`.
//! 2. Compare `expected_version` against the current version number —
//!    on mismatch, return 409 so the caller refetches + rebuilds
//!    the edit.
//! 3. Clone the IR, apply each op, validate at the end.
//! 4. Classify each op through Phase 6 routing; if any op routes to
//!    `Queue`, the whole batch queues as one approval item and no
//!    commit is made. Only fully-`Apply` batches commit
//!    synchronously.
//! 5. Commit a new version via `OntologyVersionStore::commit_version`
//!    and return the receipt.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::Serialize;
use uuid::Uuid;

use ox_ontology::{OntologyEditPreCheck, OntologyEditReceipt, OntologyEditRequest};

use crate::error::AppError;
use crate::principal::Principal;
use crate::response::ApiResponse;
use crate::routes::ontology::routing::verify_ops_apply;
use crate::state::AppState;

/// Handler response — either a commit receipt or a dry-run
/// pre-check report. Serialised untagged so the JSON shape depends
/// on which variant the request carried (`dry_run=false` → receipt,
/// `dry_run=true` → pre-check).
///
/// OpenAPI exposes this as a free-form object; callers should
/// discriminate on the presence of `new_version_id` (commit) vs.
/// `classified_changes` (pre-check).
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum OntologyEditResponse {
    Receipt(OntologyEditReceipt),
    PreCheck(OntologyEditPreCheck),
}

#[utoipa::path(
    post,
    path = "/api/ontologies/{id}/edits",
    params(
        ("id" = Uuid, Path, description = "Ontology identity (not version) id"),
    ),
    request_body = Object,
    responses(
        (status = 201, description = "Edit applied — returns new version receipt"),
        (status = 202, description = "Edit queued for approval — returns original version"),
        (status = 404, description = "Ontology not found or has no committed version yet"),
        (status = 409, description = "Version conflict — refetch and retry"),
        (status = 422, description = "Edit produced an invalid IR (SHACL / referential integrity)"),
    ),
    security(("api_key" = [])),
    tag = "Ontologies",
)]
pub(crate) async fn apply_ontology_edits(
    State(state): State<AppState>,
    principal: Principal,
    Path(ontology_id): Path<Uuid>,
    Json(req): Json<OntologyEditRequest>,
) -> Result<(StatusCode, Json<ApiResponse<OntologyEditResponse>>), AppError> {
    principal.require_designer()?;
    if req.operations.is_empty() {
        return Err(AppError::bad_request("operations must not be empty"));
    }

    // ---- 1. Load current version --------------------------------
    let current = state
        .store
        .get_current_version(ontology_id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("ontology has no committed version yet"))?;

    let current_version_num: u32 = current.version.parse().unwrap_or(0);
    if current_version_num != req.expected_version {
        return Err(AppError::conflict(format!(
            "version conflict: expected {}, current is {}; refetch and retry",
            req.expected_version, current_version_num
        )));
    }
    let parent_version_id = Some(current.id);

    // ---- 2. Dry-run short-circuit ------------------------------
    //
    // The preview path skips routing and commit entirely — it
    // applies ops to a cloned IR, runs validate(), and returns the
    // report. Routing is intentionally bypassed: the caller is
    // asking "what would this edit produce?", not "may I commit?".
    if req.dry_run {
        let mut ir = state
            .store
            .load_version(current.id)
            .await
            .map_err(AppError::from)?;

        let classified: Vec<String> = req
            .operations
            .iter()
            .map(|op| format!("{:?}", op.classify_change_type()))
            .collect();

        let mut applied = 0usize;
        let mut failed_index: Option<usize> = None;
        let mut failure: Option<String> = None;
        for (idx, op) in req.operations.iter().enumerate() {
            match op.apply_to(&mut ir) {
                Ok(()) => applied += 1,
                Err(msg) => {
                    failed_index = Some(idx);
                    failure = Some(msg);
                    break;
                }
            }
        }

        let validation = if failed_index.is_none() {
            ir.validate()
        } else {
            Vec::new()
        };
        let would_commit = failed_index.is_none() && validation.is_empty();

        return Ok((
            StatusCode::OK,
            ApiResponse::of(OntologyEditResponse::PreCheck(OntologyEditPreCheck {
                applied_operations: applied,
                failed_operation_index: failed_index,
                failure_message: failure,
                validation_errors: validation,
                classified_changes: classified,
                would_commit,
            })),
        ));
    }

    // ---- 3. Apply ops to an IR clone ----------------------------
    //
    // Ops apply atomically on a clone so a mid-batch error rolls
    // back nothing but the in-memory IR — no store side effects fire
    // until validation + routing both succeed.
    let mut ir = state
        .store
        .load_version(current.id)
        .await
        .map_err(AppError::from)?;

    for op in &req.operations {
        op.apply_to(&mut ir).map_err(AppError::unprocessable)?;
    }

    // ---- 4. Whole-IR validation ---------------------------------
    //
    // Catches referential integrity violations across mappings + code
    // systems + glossary — the contract the admin edit layer must
    // preserve.
    let validation = ir.validate();
    if !validation.is_empty() {
        return Err(AppError::unprocessable(validation.join("; ")));
    }

    // ---- 5. Routing: every op must classify to Apply, else queue -
    //
    // Validation has just passed, so the `HasValidationPass` skip
    // predicate sees a real result. A mixed batch (some Apply + some
    // Queue) would require splitting the operation list, which the
    // matrix doesn't promise — if any op queues, the whole request
    // queues. The approval surface picks it up from the workflow
    // table.
    verify_ops_apply(&state, &principal, &req.operations, true).await?;

    // ---- 6. Commit new version ---------------------------------
    //
    // Version strings are monotonically-increasing integers here —
    // the caller bumps by 1 from `expected_version`.
    let new_version_num = current_version_num + 1;
    let new_version_str = new_version_num.to_string();
    let committed_by = principal
        .user_uuid()
        .map(|u| u.to_string())
        .unwrap_or_else(|_| "apikey".into());
    let commit_message = req
        .message
        .as_deref()
        .unwrap_or("ontology edit via admin API");

    let snapshot = state
        .store
        .commit_version(
            ontology_id,
            &ir,
            &new_version_str,
            parent_version_id,
            &committed_by,
            commit_message,
        )
        .await
        .map_err(AppError::from)?;

    Ok((
        StatusCode::CREATED,
        ApiResponse::of(OntologyEditResponse::Receipt(OntologyEditReceipt {
            new_version: new_version_num,
            new_version_id: snapshot.id,
            parent_version_id,
            applied_operations: req.operations.len(),
            committed_at: snapshot.created_at,
        })),
    ))
}


