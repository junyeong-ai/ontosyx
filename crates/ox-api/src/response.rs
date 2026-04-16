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
//! ## Migration guide (Phase 1.3)
//!
//! | Before | After |
//! |--------|-------|
//! | `Ok(Json(item))` | `Ok(ApiResponse::of(item))` |
//! | `Ok(Json(page))` where `page: CursorPage<T>` | `Ok(ApiResponse::page(page))` |
//! | `Ok(StatusCode::NO_CONTENT)` | unchanged (no body) |
//! | `Ok(Json(json!({ "status": "ok" })))` | `Ok(ApiResponse::ok())` |
//!
//! `ApiResponse<T>` implements `IntoResponse` so it can be returned
//! directly from handler functions.

use axum::{Json, response::IntoResponse};
use serde::Serialize;

/// Structured response envelope for all successful API results.
///
/// Clients can always expect `response.data` to carry the primary
/// payload. `pagination` and `meta` appear only when present.
#[derive(Debug, Serialize)]
pub struct ApiResponse<T: Serialize> {
    pub data: T,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pagination: Option<PageMeta>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<serde_json::Value>,
}

/// Cursor-based pagination metadata.
#[derive(Debug, Serialize)]
pub struct PageMeta {
    pub next_cursor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total: Option<u64>,
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

    /// Attach optional metadata (e.g., model name, execution id).
    pub fn with_meta(mut self, meta: serde_json::Value) -> Self {
        self.meta = Some(meta);
        self
    }
}

impl<T: Serialize> ApiResponse<Vec<T>> {
    /// Flatten a `CursorPage<T>` into `{ "data": [...], "pagination": {...} }`.
    pub fn page(page: ox_store::CursorPage<T>) -> Json<Self> {
        Json(Self {
            data: page.items,
            pagination: Some(PageMeta {
                next_cursor: page.next_cursor,
                total: None,
            }),
            meta: None,
        })
    }

    /// Flatten a `CursorPage<T>` and include a total count.
    pub fn page_with_total(page: ox_store::CursorPage<T>, total: u64) -> Json<Self> {
        Json(Self {
            data: page.items,
            pagination: Some(PageMeta {
                next_cursor: page.next_cursor,
                total: Some(total),
            }),
            meta: None,
        })
    }
}

impl ApiResponse<serde_json::Value> {
    /// Convenience for `{ "data": { "status": "ok" } }` — used by
    /// handlers that previously returned bare `Json(json!({"status":"ok"}))`.
    pub fn ok() -> Json<Self> {
        Json(Self {
            data: serde_json::json!({ "status": "ok" }),
            pagination: None,
            meta: None,
        })
    }
}

impl<T: Serialize> IntoResponse for ApiResponse<T> {
    fn into_response(self) -> axum::response::Response {
        Json(self).into_response()
    }
}
