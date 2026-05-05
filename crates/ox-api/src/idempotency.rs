//! Idempotency-Key middleware.
//!
//! Stripe-style replay layer for mutating LLM-driven endpoints.
//! Defends against retries on transient failure double-charging
//! the LLM token bill — `POST /projects/{id}/design`,
//! `/refine`, `/edit`, and `/extend` all spend real money on
//! every call, and a network-induced retry without idempotency
//! pays that cost again.
//!
//! Wire contract:
//!
//! - Client sends `Idempotency-Key: <opaque>` on a mutating request.
//! - Server scopes the cache by `(workspace_id, user_id, method,
//!   path, key)` and hashes the request body. The first call
//!   processes normally and persists the response; later calls
//!   with the same key + matching hash replay the recorded
//!   response byte-for-byte. A reused key with a *different* body
//!   surfaces as `409 Conflict` (Stripe's behaviour) so retries
//!   can't accidentally substitute the request payload.
//! - Streaming endpoints (Content-Type starting with `text/event-stream`)
//!   bypass the cache: SSE responses cannot be replayed and the
//!   middleware refuses to stitch a half-stream into a buffered
//!   record. Callers retrying a stream must accept duplicate work.
//!
//! Layered onto a route as `axum::middleware::from_fn_with_state`.
//! Routes that don't list it are unprotected — the surface is
//! deliberately opt-in so non-LLM mutations stay simple.

use std::time::Duration;

use axum::{
    body::{Body, to_bytes},
    extract::{Request, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use chrono::Utc;
use ox_store::IdempotencyRecord;
use sha2::{Digest, Sha256};

use crate::error::AppError;
use crate::middleware::AuthClaims;
use crate::state::AppState;
use crate::workspace::WorkspaceContext;

/// Maximum body size the middleware will read and hash. Larger
/// requests are accepted but bypass idempotency; the cap protects
/// the server from a malicious caller dressing a multi-megabyte
/// body in an `Idempotency-Key` to force memory pressure.
const MAX_BUFFERED_REQUEST_BYTES: usize = 1024 * 1024;

/// Maximum response size cached. Generous enough for the LLM-driven
/// JSON responses the middleware was built to defend (design output
/// commonly hits a few hundred KB) without letting a runaway
/// streaming-disguised JSON response consume the whole table.
const MAX_BUFFERED_RESPONSE_BYTES: usize = 2 * 1024 * 1024;

/// 24-hour replay window. Long enough to cover an extended client
/// retry burst (network partition, exponential backoff) without
/// keeping every key forever.
const REPLAY_TTL: Duration = Duration::from_secs(60 * 60 * 24);

/// HTTP header carrying the caller-supplied idempotency key.
const HEADER: &str = "idempotency-key";

pub async fn idempotency_layer(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Result<Response, AppError> {
    let Some(key) = extract_key(req.headers()) else {
        return Ok(next.run(req).await);
    };

    // Streaming endpoints (`*/stream`) cannot replay byte-for-byte —
    // SSE responses are produced incrementally and the
    // request-bytes-in / response-bytes-out cache contract doesn't
    // model them. Silently bypassing the cache (record-then-skip on
    // the response side, the prior shape) gave callers a false
    // sense of safety: a retry against the same Idempotency-Key
    // hit a fresh cache miss and re-ran the full LLM cost. Reject
    // explicitly so the caller knows their retry contract for
    // streamed requests is caller-side, not header-side.
    if is_streaming_path(req.uri().path()) {
        return Err(AppError::idempotency_streaming_unsupported(
            req.uri().path().to_string(),
        ));
    }

    let principal = req
        .extensions()
        .get::<AuthClaims>()
        .cloned()
        .ok_or_else(|| {
            AppError::unauthorized(
                "Idempotency-Key requires an authenticated principal",
            )
        })?;
    let workspace = req
        .extensions()
        .get::<WorkspaceContext>()
        .cloned()
        .ok_or_else(|| AppError::workspace_header_invalid("missing"))?;
    // Synthetic API-key principals lack a UUID-shaped `sub` and
    // skip per-token revocation upstream. Idempotency cache rows
    // FK into `users.id`, so the API-key path also bypasses the
    // cache (the rare API-key caller can manage retries via the
    // automation layer that minted the key).
    if principal.is_api_key() {
        return Ok(next.run(req).await);
    }
    let user_id = principal.user_id()?;

    let method = req.method().as_str().to_owned();
    let path = req.uri().path().to_owned();

    let (parts, body) = req.into_parts();
    let body_bytes = to_bytes(body, MAX_BUFFERED_REQUEST_BYTES)
        .await
        .map_err(|_| {
            AppError::idempotency_request_body_too_large(MAX_BUFFERED_REQUEST_BYTES)
        })?;
    let request_hash = sha256(&body_bytes);

    if let Some(existing) = state
        .store
        .find_idempotency_record(
            workspace.workspace_id,
            user_id,
            &method,
            &path,
            &key,
        )
        .await
        .map_err(AppError::from)?
    {
        if existing.request_hash != request_hash {
            return Err(AppError::idempotency_key_reused());
        }
        return Ok(replay(existing));
    }

    let req = Request::from_parts(parts, Body::from(body_bytes));
    let response = next.run(req).await;

    if response.status().is_client_error() || response.status().is_server_error() {
        // Errors are not cached — Stripe's contract: caller may retry
        // an errored request with the same key and the server will
        // process it. Caching errors would lock the caller out of
        // the recovery path.
        return Ok(response);
    }
    if is_streaming(response.headers()) {
        // Defence-in-depth — the path-based reject above already
        // catches the documented streaming routes, but a future
        // handler that produces SSE without sitting on a `*/stream`
        // path would otherwise leak past the contract. Skipping the
        // record on a streamed response is correct (we cannot cache
        // a chunked payload), and the path-based reject keeps the
        // caller from forming a wrong replay assumption against it.
        return Ok(response);
    }

    let (parts, body) = response.into_parts();
    let response_bytes =
        match to_bytes(body, MAX_BUFFERED_RESPONSE_BYTES).await {
            Ok(b) => b,
            Err(_) => {
                tracing::warn!(
                    method = %method,
                    path = %path,
                    "Idempotency: response exceeded buffer cap; not cached"
                );
                return Ok(Response::from_parts(parts, Body::empty()));
            }
        };

    let content_type = parts
        .headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let now = Utc::now();
    // `from_std` only fails for durations > i64::MAX seconds; the
    // 24-hour constant is a build-time literal and the compiler
    // cannot reach the error branch without a code change here.
    let ttl = chrono::Duration::from_std(REPLAY_TTL).unwrap_or_else(|_| chrono::Duration::hours(24));
    let expires_at = now + ttl;
    let record = IdempotencyRecord {
        workspace_id: workspace.workspace_id,
        user_id,
        method: method.clone(),
        path: path.clone(),
        key: key.clone(),
        request_hash,
        response_status: parts.status.as_u16() as i16,
        response_body: response_bytes.to_vec(),
        response_content_type: content_type,
        created_at: now,
        expires_at,
    };
    if let Err(e) = state.store.create_idempotency_record(&record).await {
        // Failing to record the response only costs the caller their
        // replay protection on the next retry — it must not block
        // the original response from flowing.
        tracing::warn!(error = %e, "Idempotency: failed to record response");
    }

    Ok(Response::from_parts(parts, Body::from(response_bytes)))
}

/// `true` when the request path targets an SSE streaming endpoint —
/// every such route ends in `/stream`. Adding a new streaming route
/// follows the same suffix convention so this predicate stays stable
/// without an explicit allow-list.
fn is_streaming_path(path: &str) -> bool {
    path.ends_with("/stream")
}

fn extract_key(headers: &HeaderMap) -> Option<String> {
    let value = headers.get(HEADER)?;
    let s = value.to_str().ok()?.trim();
    if s.is_empty() || s.len() > 255 {
        return None;
    }
    Some(s.to_owned())
}

fn is_streaming(headers: &HeaderMap) -> bool {
    headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|ct| ct.starts_with("text/event-stream"))
}

fn sha256(bytes: &[u8]) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher.finalize().to_vec()
}

fn replay(record: IdempotencyRecord) -> Response {
    let status = StatusCode::from_u16(record.response_status as u16)
        .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let mut response = (status, record.response_body).into_response();
    if let Some(ct) = record.response_content_type
        && let Ok(value) = HeaderValue::from_str(&ct)
    {
        response.headers_mut().insert(header::CONTENT_TYPE, value);
    }
    response
        .headers_mut()
        .insert("idempotent-replay", HeaderValue::from_static("true"));
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_key_rejects_empty_string() {
        let mut headers = HeaderMap::new();
        headers.insert(HEADER, HeaderValue::from_static(""));
        assert!(extract_key(&headers).is_none());
    }

    #[test]
    fn extract_key_trims_whitespace() {
        let mut headers = HeaderMap::new();
        headers.insert(HEADER, HeaderValue::from_static("  abc  "));
        assert_eq!(extract_key(&headers).as_deref(), Some("abc"));
    }

    #[test]
    fn extract_key_rejects_overlong_values() {
        let mut headers = HeaderMap::new();
        let too_long: String = std::iter::repeat_n('a', 256).collect();
        headers
            .insert(HEADER, HeaderValue::from_str(&too_long).unwrap());
        assert!(extract_key(&headers).is_none());
    }

    #[test]
    fn is_streaming_matches_event_stream_prefix() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("text/event-stream; charset=utf-8"),
        );
        assert!(is_streaming(&headers));
    }

    #[test]
    fn is_streaming_rejects_application_json() {
        let mut headers = HeaderMap::new();
        headers
            .insert(header::CONTENT_TYPE, HeaderValue::from_static("application/json"));
        assert!(!is_streaming(&headers));
    }

    #[test]
    fn sha256_is_deterministic_per_payload() {
        let a = sha256(b"hello");
        let b = sha256(b"hello");
        assert_eq!(a, b);
        assert_ne!(a, sha256(b"hello!"));
    }

    #[test]
    fn is_streaming_path_matches_documented_routes() {
        assert!(is_streaming_path("/api/ontology-drafts/abc/design/stream"));
        assert!(is_streaming_path("/api/ontology-drafts/abc/refine/stream"));
        assert!(is_streaming_path("/api/chat/stream"));
    }

    #[test]
    fn is_streaming_path_rejects_non_streaming_routes() {
        assert!(!is_streaming_path("/api/ontology-drafts/abc/design"));
        assert!(!is_streaming_path("/api/ontology-drafts/abc/refine"));
        assert!(!is_streaming_path("/api/streaming-config"));
        assert!(!is_streaming_path("/api/chat"));
    }
}
