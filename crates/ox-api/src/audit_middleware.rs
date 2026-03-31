// ---------------------------------------------------------------------------
// Audit Middleware — automatic audit logging for all mutation endpoints
// ---------------------------------------------------------------------------
// Cross-cutting concern: records every POST/PUT/PATCH/DELETE request
// to the audit log after the handler completes. GET requests are skipped.
//
// This is applied as a route_layer on the protected router, so it runs
// inside the auth + workspace context middlewares and has access to
// AuthClaims and WorkspaceContext in request extensions.
// ---------------------------------------------------------------------------

use std::time::Instant;

use axum::{extract::{Request, State}, middleware::Next, response::Response};
use axum::http::Method;
use serde_json::json;
use uuid::Uuid;

use crate::middleware::AuthClaims;
use crate::state::AppState;

/// Audit middleware — logs all successful mutation requests automatically.
///
/// Runs after require_auth + workspace_context, before the handler response
/// is sent to the client. Only records 2xx/3xx mutations; skips reads and errors.
pub async fn audit_log(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Response {
    let method = req.method().clone();

    // Skip GET/HEAD/OPTIONS — read operations don't need audit
    if matches!(method, Method::GET | Method::HEAD | Method::OPTIONS) {
        return next.run(req).await;
    }

    let path = req.uri().path().to_string();
    let start = Instant::now();

    // Extract identity from extensions (set by require_auth middleware)
    let user_id = req
        .extensions()
        .get::<AuthClaims>()
        .and_then(|c| Uuid::parse_str(&c.sub).ok());

    let response = next.run(req).await;

    let status = response.status().as_u16();
    let duration_ms = start.elapsed().as_millis() as u64;

    // Only audit successful mutations (2xx/3xx)
    if status >= 400 {
        return response;
    }

    // Fire-and-forget audit recording.
    // Must use spawn_scoped to propagate workspace task-locals (WORKSPACE_ID)
    // because tokio::spawn loses them and RLS blocks the INSERT.
    let store = state.store.clone();
    let action = format!("{} {}", method.as_str(), &path);
    let resource_type = extract_resource_type(&path);
    let details = json!({
        "method": method.as_str(),
        "path": path,
        "status": status,
        "duration_ms": duration_ms,
    });

    crate::spawn_scoped::spawn_scoped(async move {
        let _ = store
            .record_audit(user_id, &action, &resource_type, None, details)
            .await;
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
fn singularize(word: &str) -> String {
    if word.ends_with("ies") {
        // policies → policy, entries → entry
        format!("{}y", &word[..word.len() - 3])
    } else if word.ends_with("ses") || word.ends_with("xes") {
        // addresses → address, indexes → index
        word[..word.len() - 2].to_string()
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
        assert_eq!(extract_resource_type("/api/projects"), "project");
    }
}
