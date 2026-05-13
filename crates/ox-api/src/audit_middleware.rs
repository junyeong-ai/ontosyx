// ---------------------------------------------------------------------------
// Audit Middleware — automatic audit logging for all mutation endpoints
// ---------------------------------------------------------------------------
// Cross-cutting concern: records every POST/PUT/PATCH/DELETE request
// to the audit log after the handler completes, regardless of outcome.
// Read-only methods (GET/HEAD/OPTIONS) are skipped.
//
// Failed mutations are recorded too — without this, an attacker probing
// for unauthorised endpoints leaves no trace, and a permission-denied
// event on a sensitive resource is invisible to post-incident forensics.
// The `success` field on the stored `details` payload distinguishes
// `2xx/3xx` outcomes from `4xx/5xx` so queries can filter either class.
//
// Applied as a route_layer on the protected router: runs inside
// require_auth + workspace_context, so AuthClaims and WorkspaceContext
// are available in request extensions.
// ---------------------------------------------------------------------------

use std::time::Instant;

use axum::http::Method;
use axum::{
    extract::{Request, State},
    middleware::Next,
    response::Response,
};
use serde_json::json;
use uuid::Uuid;

use crate::middleware::AuthClaims;
use crate::state::AppState;

/// Audit middleware — records every mutation request automatically.
///
/// Runs after require_auth + workspace_context, before the handler response
/// returns to the client. Records 2xx/3xx *and* 4xx/5xx mutations; the
/// `success` field in the stored `details` distinguishes them. Only
/// read-only methods (GET/HEAD/OPTIONS) are skipped entirely.
pub async fn audit_log(State(state): State<AppState>, req: Request, next: Next) -> Response {
    let method = req.method().clone();

    if matches!(method, Method::GET | Method::HEAD | Method::OPTIONS) {
        return next.run(req).await;
    }

    let path = req.uri().path().to_string();
    let start = Instant::now();

    let user_id = req
        .extensions()
        .get::<AuthClaims>()
        .and_then(|c| Uuid::parse_str(&c.sub).ok());

    let response = next.run(req).await;

    let status = response.status().as_u16();
    let duration_ms = start.elapsed().as_millis() as u64;
    let success = (200..400).contains(&status);

    let store = state.store.clone();
    let request_id = Uuid::new_v4();
    let action = format!("{} {}", method.as_str(), &path);
    let resource_type = extract_resource_type(&path);
    let details = json!({
        "request_id": request_id,
        "method": method.as_str(),
        "path": path,
        "status": status,
        "duration_ms": duration_ms,
        "success": success,
    });

    // Fire-and-forget. spawn_scoped propagates WORKSPACE_ID into the
    // spawned future — tokio::spawn would drop it and RLS would reject
    // the audit INSERT.
    ox_context::spawn_scoped(async move {
        if let Err(error) = store
            .record_audit(user_id, &action, &resource_type, None, details)
            .await
        {
            tracing::warn!(?error, %action, %resource_type, "audit record failed");
        }
    });

    response
}

/// Extract the primary resource type from the API path.
/// e.g., "/api/dashboards/uuid/widgets" → "dashboard.widget"
fn extract_resource_type(path: &str) -> String {
    let segments: Vec<&str> = path
        .trim_start_matches("/api/")
        .split('/')
        .filter(|s| !s.is_empty() && Uuid::parse_str(s).is_err())
        .collect();

    match segments.as_slice() {
        [] => "unknown".to_string(),
        [resource] => singularize(resource).to_string(),
        [resource, sub] => {
            format!("{}.{}", singularize(resource), singularize(sub))
        }
        [resource, _, sub, ..] => {
            format!("{}.{}", singularize(resource), singularize(sub))
        }
    }
}

/// Naive English singularization for resource path segments.
/// Resource paths use hyphens for compound nouns (`ontology-drafts`)
/// per REST convention; the audit log normalises to snake_case for
/// downstream consumers, so the helper converts on the way through.
fn singularize(word: &str) -> String {
    let word = &word.replace('-', "_");
    if let Some(stem) = word.strip_suffix("ies") {
        // policies → policy, entries → entry
        format!("{stem}y")
    } else if let Some(stem) = word.strip_suffix("ses") {
        // addresses → address
        stem.to_string()
    } else if let Some(stem) = word.strip_suffix("xes") {
        // indexes → index
        stem.to_string()
    } else if word.ends_with('s')
        && !word.ends_with("ss")
        && !word.ends_with("us")
        && !word.ends_with("is")
    {
        // dashboards → dashboard, widgets → widget
        word[..word.len() - 1].to_string()
    } else {
        word.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_resource_type() {
        assert_eq!(extract_resource_type("/api/dashboards"), "dashboard");
        assert_eq!(
            extract_resource_type("/api/dashboards/550e8400-e29b-41d4-a716-446655440000/widgets"),
            "dashboard.widget"
        );
        assert_eq!(extract_resource_type("/api/workspaces"), "workspace");
        assert_eq!(
            extract_resource_type("/api/workspaces/550e8400-e29b-41d4-a716-446655440000/members"),
            "workspace.member"
        );
        assert_eq!(extract_resource_type("/api/quality/rules"), "quality.rule");
        assert_eq!(extract_resource_type("/api/acl/policies"), "acl.policy");
        assert_eq!(
            extract_resource_type("/api/ontology-drafts"),
            "ontology_draft"
        );
    }
}
