//! Typed mapper from [`entelix::Error`] to [`OxError::Llm`].
//!
//! Every layer boundary that calls into entelix folds the SDK error
//! through this mapper. The mapper preserves the typed classification
//! ([`LlmErrorCode`]) so the API boundary can render a stable
//! [`ApiErrorCode::Llm*`](#) per variant — the FE i18n catalogue at
//! `errors.llm_<code>` produces the user-facing prose.
//!
//! The classifier routes through entelix's
//! [`Error::wire_code`](entelix::Error::wire_code) — a
//! patch-version-stable `&'static str` that buckets HTTP families and
//! transport-class failures into named buckets — and folds the
//! 17-bucket SDK taxonomy onto the 11 LLM codes Ontosyx surfaces. New
//! entelix variants land in the wildcard arm with a `tracing::warn!`
//! so operators see unmapped buckets in the next minor sweep.
//!
//! Pair with [`OxError::with_context`] when crossing further layer
//! boundaries (e.g. brain → api): `with_context` carries the
//! `target/location` axis, this helper carries the operator-provided
//! prefix on the detail string.

use ox_core::error::{LlmErrorCode, OxError};
use tracing::warn;

/// Returns a closure that maps [`entelix::Error`] → [`OxError::Llm`]
/// with `context` as the prefix on the diagnostic detail string.
///
/// ```ignore
/// chat
///     .complete_request(request, ctx)
///     .await
///     .map_err(map_entelix_err("LLM request failed"))?;
/// ```
///
/// Accepts any `Into<String>` so callers can pass a literal
/// (`map_entelix_err("...")`) or a dynamic prefix
/// (`map_entelix_err(format!("workspace {ws} embedding"))`).
pub fn map_entelix_err(context: impl Into<String>) -> impl FnOnce(entelix::Error) -> OxError {
    let context = context.into();
    move |err| {
        let (code, retry_after_secs) = classify(&err);
        OxError::Llm {
            code,
            detail: format!("{context}: {err}"),
            retry_after_secs,
        }
    }
}

/// Project an entelix error onto the typed [`LlmErrorCode`] +
/// optional `Retry-After` hint via [`entelix::Error::envelope`].
/// The envelope bundles the patch-version-stable wire bucket with
/// the vendor `Retry-After` hint in one `Copy` value; adding a new
/// wire-code in a future entelix minor lands in the wildcard arm of
/// [`classify_wire_code`] with a `tracing::warn!`, so the typed
/// surface keeps a single source of truth upstream.
fn classify(err: &entelix::Error) -> (LlmErrorCode, Option<u64>) {
    let envelope = err.envelope();
    (
        classify_wire_code(envelope.wire_code),
        envelope.retry_after_secs,
    )
}

/// Project an [`entelix::ErrorEnvelope::wire_code`] bucket onto the
/// typed [`LlmErrorCode`]. Shared by every call site that holds the
/// wire bucket but not the typed error itself — agent sink consumers
/// receive `AgentEvent::Failed { envelope, .. }` and route
/// classification through this helper so the agent-loop failure
/// envelope and the synchronous HTTP error envelope key off the same
/// `LlmErrorCode` set, never two parallel namespaces.
pub fn classify_wire_code(wire_code: &str) -> LlmErrorCode {
    match wire_code {
        "invalid_request" | "upstream_invalid" => LlmErrorCode::InvalidRequest,
        "config_error" => LlmErrorCode::InvalidRequest,
        "rate_limited" => LlmErrorCode::RateLimited,
        "upstream_unauthorized" | "auth_failed" => LlmErrorCode::AuthFailed,
        "transport_failure" | "tls_failure" | "dns_failure" => LlmErrorCode::Transient,
        "upstream_unavailable" | "upstream_error" => LlmErrorCode::ProviderUnavailable,
        "cancelled" => LlmErrorCode::Cancelled,
        "deadline_exceeded" => LlmErrorCode::DeadlineExceeded,
        "interrupted" => LlmErrorCode::Interrupted,
        "model_retry_exhausted" => LlmErrorCode::ModelRetry,
        "serde" => LlmErrorCode::SerializationError,
        "quota_exceeded" => LlmErrorCode::BudgetExceeded,
        other => {
            warn!(
                entelix_wire_code = other,
                "entelix `Error::wire_code` returned an unmapped bucket — \
                 falling back to LlmErrorCode::ProviderUnavailable"
            );
            LlmErrorCode::ProviderUnavailable
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn extract(err: OxError) -> (LlmErrorCode, String, Option<u64>) {
        match err {
            OxError::Llm {
                code,
                detail,
                retry_after_secs,
            } => (code, detail, retry_after_secs),
            other => panic!("expected OxError::Llm, got {other:?}"),
        }
    }

    #[test]
    fn classifies_invalid_request() {
        let map = map_entelix_err("phase X");
        let (code, detail, _) = extract(map(entelix::Error::invalid_request("empty messages")));
        assert_eq!(code, LlmErrorCode::InvalidRequest);
        assert!(detail.starts_with("phase X: "));
        assert!(detail.contains("empty messages"));
    }

    #[test]
    fn classifies_config_as_invalid_request() {
        let map = map_entelix_err("build");
        let (code, _, _) = extract(map(entelix::Error::config("bad base_url")));
        assert_eq!(code, LlmErrorCode::InvalidRequest);
    }

    #[test]
    fn classifies_rate_limit_with_retry_after() {
        let map = map_entelix_err("call");
        let err = entelix::Error::provider_http(429, "Too many requests")
            .with_retry_after(std::time::Duration::from_secs(7));
        let (code, _, retry) = extract(map(err));
        assert_eq!(code, LlmErrorCode::RateLimited);
        assert_eq!(retry, Some(7));
    }

    #[test]
    fn classifies_network_as_transient() {
        let map = map_entelix_err("call");
        let err = entelix::Error::provider_network("connection refused");
        let (code, _, _) = extract(map(err));
        assert_eq!(code, LlmErrorCode::Transient);
    }

    #[test]
    fn classifies_tls_as_transient() {
        let map = map_entelix_err("call");
        let err = entelix::Error::provider_tls("handshake failure");
        let (code, _, _) = extract(map(err));
        assert_eq!(code, LlmErrorCode::Transient);
    }

    #[test]
    fn classifies_5xx_as_provider_unavailable() {
        let map = map_entelix_err("call");
        let err = entelix::Error::provider_http(503, "service unavailable");
        let (code, _, _) = extract(map(err));
        assert_eq!(code, LlmErrorCode::ProviderUnavailable);
    }

    #[test]
    fn classifies_401_as_auth_failed() {
        let map = map_entelix_err("call");
        let err = entelix::Error::provider_http(401, "bad bearer");
        let (code, _, _) = extract(map(err));
        assert_eq!(code, LlmErrorCode::AuthFailed);
    }

    #[test]
    fn classifies_403_as_auth_failed() {
        let map = map_entelix_err("call");
        let err = entelix::Error::provider_http(403, "forbidden");
        let (code, _, _) = extract(map(err));
        assert_eq!(code, LlmErrorCode::AuthFailed);
    }

    #[test]
    fn classifies_400_as_invalid_request() {
        let map = map_entelix_err("call");
        let err = entelix::Error::provider_http(400, "malformed");
        let (code, _, _) = extract(map(err));
        assert_eq!(code, LlmErrorCode::InvalidRequest);
    }

    #[test]
    fn classifies_non_carved_4xx_as_invalid_request() {
        // entelix `wire_code` carves 429 / 401 / 403 out and buckets
        // every other 4xx into `upstream_invalid`. Any provider-side
        // "context too long" surface lands here — the wire bucket is
        // intentionally coarse so the FE catalogue owns the prose.
        let map = map_entelix_err("call");
        let err = entelix::Error::provider_http(413, "payload too large");
        let (code, _, _) = extract(map(err));
        assert_eq!(code, LlmErrorCode::InvalidRequest);
    }

    #[test]
    fn classifies_cancelled() {
        let map = map_entelix_err("call");
        let (code, _, _) = extract(map(entelix::Error::Cancelled));
        assert_eq!(code, LlmErrorCode::Cancelled);
    }

    #[test]
    fn classifies_deadline_exceeded() {
        let map = map_entelix_err("call");
        let (code, _, _) = extract(map(entelix::Error::DeadlineExceeded));
        assert_eq!(code, LlmErrorCode::DeadlineExceeded);
    }

    #[test]
    fn accepts_owned_dynamic_context() {
        let dynamic = format!("workspace {} embedding", "abc");
        let map = map_entelix_err(dynamic);
        let (_, detail, _) = extract(map(entelix::Error::provider_network("connection refused")));
        assert!(detail.starts_with("workspace abc embedding: "));
        assert!(detail.contains("connection refused"));
    }
}
