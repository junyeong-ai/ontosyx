//! # Uniform API response envelope
//!
//! All successful responses from the HTTP API use `ApiResponse<T>`:
//!
//! ```json
//! {
//!   "data": <T>,
//!   "pagination": { "next_cursor": "...", "total": 42 },  // optional
//!   "meta": { "model": "claude-sonnet-4-6" }              // optional
//! }
//! ```
//!
//! Error responses are handled separately by [`crate::error::AppError`],
//! which already emits `{ "error": { "type": "...", "message": "..." } }`.
//!
//! ## Migration guide
//!
//! | Before | After |
//! |--------|-------|
//! | `Ok(Json(item))` | `Ok(ApiResponse::of(item))` |
//! | `Ok(Json(page))` where `page: CursorPage<T>` | `Ok(ApiResponse::page(page))` |
//! | `Ok(StatusCode::NO_CONTENT)` | unchanged (no body) |
//! | `Ok(Json(json!({ "status": "ok" })))` | `Ok(StatusCode::NO_CONTENT)` (drop the empty body) |
//!
//! `ApiResponse<T>` implements `IntoResponse` so it can be returned
//! directly from handler functions.

use axum::{Json, response::IntoResponse};
use serde::Serialize;
use utoipa::ToSchema;

/// Structured response envelope for all successful API results.
///
/// Wire shape (from `Serialize`):
/// ```json
/// { "data": <T>, "pagination": {...}?, "meta": {...}? }
/// ```
///
/// ### OpenAPI / generated-client contract (READ THIS)
///
/// **Every successful 2xx body from this server is wrapped in this
/// envelope.** A handler's `#[utoipa::path(responses(... body = T))]`
/// describes the type that lives at `data`, *not* the wire shape.
///
/// Generated clients (e.g. via `openapi-typescript`) should either:
///   1. add a post-processing step that unwraps `data` — this is what
///      `web/src/lib/api/client.ts` does, so frontend callers in this
///      repo receive `T` directly; or
///   2. wrap every response type in the
///      [`crate::response::PageMeta`]-style envelope when consuming
///      the raw OpenAPI spec from a third-party codegen.
///
/// We deliberately do *not* derive `ToSchema` on `ApiResponse<T>`:
/// a generic `T: ToSchema` bound would cascade onto every handler's
/// return type and force `ToSchema` on internal serde-only structs.
/// Instead the envelope is documented here and via the OpenAPI root
/// description; the per-handler `body = T` line stays accurate for
/// the *payload* type.
#[derive(Debug, Serialize)]
pub struct ApiResponse<T: Serialize> {
    pub data: T,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pagination: Option<PageMeta>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<serde_json::Value>,
}

/// Cursor-based pagination metadata.
///
/// Carries the opaque cursor used to fetch the next page of a
/// cursor-paginated list endpoint. Surfaces in OpenAPI under
/// `components.schemas.PageMeta`; list-handler responses include this
/// inside the universal `ApiResponse` envelope.
#[derive(Debug, Serialize, ToSchema)]
pub struct PageMeta {
    /// Opaque cursor for the next page. `None` when this is the last page.
    pub next_cursor: Option<String>,
}

impl<T: Serialize> ApiResponse<T> {
    /// Wrap a single value as `{ "data": T }`.
    pub fn of(data: T) -> Json<Self> {
        Json(Self {
            data,
            pagination: None,
            meta: None,
        })
    }
}

impl<T: Serialize> ApiResponse<Vec<T>> {
    /// Flatten a `CursorPage<T>` into `{ "data": [...], "pagination": {...} }`.
    pub fn page(page: ox_store::CursorPage<T>) -> Json<Self> {
        Json(Self {
            data: page.items,
            pagination: Some(PageMeta {
                next_cursor: page.next_cursor,
            }),
            meta: None,
        })
    }
}

impl<T: Serialize> IntoResponse for ApiResponse<T> {
    fn into_response(self) -> axum::response::Response {
        Json(self).into_response()
    }
}
