use std::time::{Duration, Instant};

use axum::{
    extract::{Request, State},
    http::{HeaderMap, HeaderValue},
    middleware::Next,
    response::Response,
};
use dashmap::DashMap;
use jsonwebtoken::{Algorithm, DecodingKey, Validation};
use serde::{Deserialize, Serialize};
use subtle::ConstantTimeEq;
use uuid::Uuid;

use crate::config::RateLimitConfig;
use crate::error::AppError;
use crate::state::AppState;

// ---------------------------------------------------------------------------
// JWT claims
// ---------------------------------------------------------------------------

/// Claims embedded in platform JWTs.
/// Created by the `/auth/token` endpoint after OIDC verification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthClaims {
    /// User UUID (from `users.id`)
    pub sub: String,
    pub email: String,
    pub name: Option<String>,
    /// "admin", "designer", "viewer"
    pub role: String,
    /// JWT issuer (always "ontosyx")
    pub iss: String,
    /// Expiration (UNIX timestamp)
    pub exp: usize,
    /// Issued at (UNIX timestamp)
    pub iat: usize,
}

impl AuthClaims {
    /// Parse the `sub` field as a UUID.
    #[allow(dead_code)]
    pub fn user_id(&self) -> Result<Uuid, AppError> {
        Uuid::parse_str(&self.sub).map_err(|_| AppError::unauthorized("Invalid user ID in token"))
    }
}

// ---------------------------------------------------------------------------
// API key authentication (kept for programmatic/CI access)
// ---------------------------------------------------------------------------

/// Try API key authentication from `X-API-Key` header.
/// Returns `Ok(())` if valid, `Err` otherwise.
fn try_api_key_auth(headers: &HeaderMap, expected_key: Option<&str>) -> Result<(), AppError> {
    let expected = expected_key.ok_or_else(|| AppError::unauthorized("API key not configured"))?;

    let provided = headers
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| AppError::unauthorized("Missing API key"))?;

    if provided.as_bytes().ct_eq(expected.as_bytes()).into() {
        Ok(())
    } else {
        Err(AppError::unauthorized("Invalid API key"))
    }
}

// ---------------------------------------------------------------------------
// JWT authentication
// ---------------------------------------------------------------------------

/// Extract a JWT token from the request (`Authorization: Bearer` header
/// or `ontosyx_session` cookie).
fn extract_token(req: &Request) -> Option<String> {
    // Try Authorization header first (used by BFF proxy)
    if let Some(auth) = req.headers().get("authorization")
        && let Ok(value) = auth.to_str()
        && let Some(token) = value.strip_prefix("Bearer ")
    {
        return Some(token.to_string());
    }

    // Try cookie (direct browser access, if applicable)
    if let Some(cookie_header) = req.headers().get("cookie")
        && let Ok(cookies) = cookie_header.to_str()
    {
        for cookie in cookies.split(';') {
            let cookie = cookie.trim();
            if let Some(token) = cookie.strip_prefix("ontosyx_session=") {
                return Some(token.to_string());
            }
        }
    }

    None
}

/// Validate a platform JWT and return the embedded claims.
pub(crate) fn validate_jwt(token: &str, secret: &str) -> Result<AuthClaims, AppError> {
    let key = DecodingKey::from_secret(secret.as_bytes());
    let mut validation = Validation::new(Algorithm::HS256);
    validation.validate_exp = true;
    validation.set_issuer(&["ontosyx"]);
    validation.required_spec_claims = ["sub", "exp", "iat"]
        .iter()
        .map(|s| s.to_string())
        .collect();

    let token_data = jsonwebtoken::decode::<AuthClaims>(token, &key, &validation).map_err(|e| {
        tracing::debug!(error = %e, "JWT validation failed");
        AppError::unauthorized("Invalid or expired token")
    })?;

    Ok(token_data.claims)
}

/// Create a platform JWT for a user.
pub fn create_jwt(claims: &AuthClaims, secret: &str) -> Result<String, AppError> {
    let key = jsonwebtoken::EncodingKey::from_secret(secret.as_bytes());
    jsonwebtoken::encode(&jsonwebtoken::Header::new(Algorithm::HS256), claims, &key)
        .map_err(|e| AppError::internal(format!("Failed to create JWT: {e}")))
}

// ---------------------------------------------------------------------------
// Auth middleware: JWT first, API key fallback
// ---------------------------------------------------------------------------

/// Authentication middleware for protected endpoints.
///
/// Tries JWT authentication first (cookie or Authorization header), then falls
/// back to API key authentication for programmatic/CI access.
///
/// On successful JWT auth, injects `AuthClaims` into request extensions.
/// On successful API key auth, injects a synthetic `AuthClaims` for the
/// system principal.
pub async fn require_auth(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Result<Response, AppError> {
    // Try JWT auth
    if let Some(ref secret) = state.auth_config.jwt_secret
        && let Some(token) = extract_token(&req)
    {
        let claims = validate_jwt(&token, secret)?;
        req.extensions_mut().insert(claims);
        return Ok(next.run(req).await);
    }

    // Fall back to API key auth
    if try_api_key_auth(req.headers(), state.auth_config.api_key.as_deref()).is_ok() {
        // Inject a synthetic claims object for API key access
        let claims = AuthClaims {
            sub: "system:api-key".to_string(),
            email: "system@ontosyx.local".to_string(),
            name: Some("API Key".to_string()),
            role: "admin".to_string(),
            iss: "ontosyx-api-key".to_string(),
            exp: usize::MAX,
            iat: 0,
        };
        req.extensions_mut().insert(claims);
        return Ok(next.run(req).await);
    }

    // Neither auth method succeeded
    let has_jwt_secret = state.auth_config.jwt_secret.is_some();
    let has_api_key = state.auth_config.api_key.is_some();

    if !has_jwt_secret && !has_api_key {
        tracing::warn!("Auth request rejected: neither JWT secret nor API key configured");
        Err(AppError::service_unavailable(
            "Authentication not configured. Set OX_AUTH__JWT_SECRET or OX_AUTH__API_KEY.",
        ))
    } else {
        Err(AppError::unauthorized(
            "Invalid or missing authentication. Provide a valid JWT or API key.",
        ))
    }
}

// ---------------------------------------------------------------------------
// Helpers for extracting claims in handlers
// ---------------------------------------------------------------------------

/// Get the authenticated user's ID as a string from request extensions.
/// Returns "system" for API key auth.
#[allow(dead_code)]
pub fn get_user_id_from_claims(req: &Request) -> String {
    req.extensions()
        .get::<AuthClaims>()
        .map(|c| c.sub.clone())
        .unwrap_or_else(|| "anonymous".to_string())
}

// ---------------------------------------------------------------------------
// Request ID middleware
//
// Generates a UUID at request arrival time and propagates it on the response
// via `x-request-id` header for log correlation.
// Preserves client-provided `x-request-id` if present.
// ---------------------------------------------------------------------------

pub async fn inject_request_id(request: Request, next: Next) -> Response {
    let request_id = request
        .headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    let mut response = next.run(request).await;

    if let Ok(value) = HeaderValue::from_str(&request_id) {
        response.headers_mut().insert("x-request-id", value);
    }

    response
}

// ---------------------------------------------------------------------------
// Per-user rate limiting (fixed-window counter)
//
// Each user gets `requests_per_window` requests per `window_secs` window.
// Uses a DashMap for concurrent per-user tracking. Expired entries are
// cleaned up lazily on access and periodically via a background task.
//
// User is identified by the `sub` claim from JWT auth, or the `x-api-key`
// header identity. Unauthenticated requests use a shared "anonymous" bucket.
// ---------------------------------------------------------------------------

const ANONYMOUS_USER: &str = "__anonymous__";

/// Per-user fixed-window counter entry.
struct WindowEntry {
    /// Start of the current window.
    window_start: Instant,
    /// Number of requests in the current window.
    count: u32,
}

/// In-process per-user rate limiter using fixed-window counters.
pub struct RateLimiter {
    /// Per-user counters. Key = user id.
    counters: DashMap<String, WindowEntry>,
    /// Maximum requests allowed per window.
    max_requests: u32,
    /// Window duration.
    window: Duration,
}

impl RateLimiter {
    pub fn new(config: &RateLimitConfig) -> Self {
        Self {
            counters: DashMap::new(),
            max_requests: config.requests_per_window,
            window: Duration::from_secs(config.window_secs),
        }
    }

    /// Check and increment the counter for the given user.
    /// Returns `Ok(remaining)` on success, or `Err(retry_after_secs)` if the limit is exceeded.
    fn check(&self, user: &str) -> Result<u32, u64> {
        let now = Instant::now();

        let mut entry = self
            .counters
            .entry(user.to_owned())
            .or_insert_with(|| WindowEntry {
                window_start: now,
                count: 0,
            });

        let elapsed = now.duration_since(entry.window_start);

        // Window expired — reset
        if elapsed >= self.window {
            entry.window_start = now;
            entry.count = 1;
            return Ok(self.max_requests.saturating_sub(1));
        }

        if entry.count >= self.max_requests {
            let retry_after = self.window.saturating_sub(elapsed).as_secs().max(1);
            return Err(retry_after);
        }

        entry.count += 1;
        Ok(self.max_requests.saturating_sub(entry.count))
    }

    /// Remove entries whose window has expired. Called periodically from a background task.
    fn cleanup(&self) {
        let now = Instant::now();
        self.counters
            .retain(|_, entry| now.duration_since(entry.window_start) < self.window);
    }

    /// Spawn a background task that periodically cleans up expired entries.
    /// Participates in graceful shutdown via the provided cancellation token.
    pub fn spawn_cleanup_task(
        self: &std::sync::Arc<Self>,
        cancel_token: tokio_util::sync::CancellationToken,
    ) {
        let limiter = std::sync::Arc::clone(self);
        let interval = limiter.window;
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(interval);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! {
                    _ = cancel_token.cancelled() => {
                        tracing::info!("Shutting down rate limiter cleanup task");
                        break;
                    }
                    _ = ticker.tick() => {
                        limiter.cleanup();
                    }
                }
            }
        });
    }
}

/// Rate limiting middleware.
///
/// Extracts the user identity from `AuthClaims` (if present in extensions,
/// set by `require_auth` middleware) or falls back to a shared anonymous bucket.
/// Returns 429 Too Many Requests with `Retry-After` header when the limit
/// is exceeded.
pub async fn rate_limit(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Result<Response, AppError> {
    let limiter = match &state.rate_limiter {
        Some(rl) => rl,
        None => return Ok(next.run(request).await),
    };

    let user = request
        .extensions()
        .get::<AuthClaims>()
        .map(|c| c.sub.as_str())
        .unwrap_or(ANONYMOUS_USER);

    match limiter.check(user) {
        Ok(remaining) => {
            let mut response = next.run(request).await;
            // Inform clients of their remaining budget
            if let Ok(v) = HeaderValue::from_str(&remaining.to_string()) {
                response.headers_mut().insert("x-ratelimit-remaining", v);
            }
            if let Ok(v) = HeaderValue::from_str(&limiter.max_requests.to_string()) {
                response.headers_mut().insert("x-ratelimit-limit", v);
            }
            Ok(response)
        }
        Err(retry_after) => {
            crate::metrics::record_rate_limit_exceeded();
            tracing::warn!(
                user = user,
                retry_after_secs = retry_after,
                "Rate limit exceeded"
            );
            Err(AppError::rate_limited(retry_after))
        }
    }
}

// ---------------------------------------------------------------------------
// Workspace context middleware
// ---------------------------------------------------------------------------
// Runs after `require_auth`. Resolves the workspace for the request:
//   1. Read `X-Workspace-Id` header (optional).
//   2. If absent, fall back to the user's default workspace.
//   3. Verify the user is a member.
//   4. Inject `WorkspaceContext` into request extensions.
//   5. Set `WORKSPACE_ID` task-local so PgPool `before_acquire` can set
//      the session variable for RLS.
// ---------------------------------------------------------------------------

pub async fn workspace_context(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Result<Response, AppError> {
    use crate::workspace::{WorkspaceContext, WorkspaceRole};
    use ox_runtime::{GRAPH_SYSTEM_BYPASS, GRAPH_WORKSPACE_ID};
    use ox_store::WORKSPACE_ID;

    // Requires auth claims (must run after require_auth)
    let claims = req
        .extensions()
        .get::<AuthClaims>()
        .cloned()
        .ok_or_else(|| AppError::unauthorized("Authentication required"))?;

    // API key users: if X-Workspace-Id is provided, scope to that workspace
    // (same as JWT users). Otherwise, use SYSTEM_BYPASS for cross-workspace access.
    if claims.sub.starts_with("system:") {
        let explicit_workspace = req
            .headers()
            .get("x-workspace-id")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| Uuid::parse_str(s).ok());

        if let Some(workspace_id) = explicit_workspace {
            // Explicit workspace: scope to it (no bypass)
            let ws_ctx = WorkspaceContext {
                workspace_id,
                workspace_role: WorkspaceRole::Owner,
            };
            req.extensions_mut().insert(ws_ctx);
            let response = WORKSPACE_ID
                .scope(
                    workspace_id,
                    GRAPH_WORKSPACE_ID.scope(workspace_id, next.run(req)),
                )
                .await;
            return Ok(response);
        } else {
            // No workspace specified: system bypass (all data visible)
            let ws_ctx = WorkspaceContext {
                workspace_id: Uuid::nil(),
                workspace_role: WorkspaceRole::Owner,
            };
            req.extensions_mut().insert(ws_ctx);
            let response = ox_store::SYSTEM_BYPASS
                .scope(true, GRAPH_SYSTEM_BYPASS.scope(true, next.run(req)))
                .await;
            return Ok(response);
        }
    }

    let user_id: Uuid = claims.user_id()?;

    // Resolve workspace ID
    let workspace_id = if let Some(header) = req.headers().get("x-workspace-id") {
        let id_str = header
            .to_str()
            .map_err(|_| AppError::bad_request("Invalid X-Workspace-Id header"))?;
        Uuid::parse_str(id_str)
            .map_err(|_| AppError::bad_request("X-Workspace-Id must be a valid UUID"))?
    } else {
        // Fall back to default workspace
        let ws = state
            .store
            .get_default_workspace(user_id)
            .await
            .map_err(|e| AppError::internal(format!("Failed to resolve workspace: {e}")))?;
        match ws {
            Some(w) => w.id,
            None => {
                return Err(AppError::bad_request(
                    "No workspace found. Create a workspace first.",
                ));
            }
        }
    };

    // Verify membership.
    //
    // NOTE: There is a microsecond-level race between get_default_workspace()
    // and get_member_role() — a user could be removed between the two calls.
    // This is acceptable because:
    //   1. PostgreSQL RLS is the true enforcement boundary, not this middleware check.
    //   2. If the user was removed, any mutating store operation will be denied by RLS.
    //   3. The next request will fail at get_member_role (consistent eventual denial).
    let role = state
        .store
        .get_member_role(workspace_id, user_id)
        .await
        .map_err(|e| AppError::internal(format!("Failed to check membership: {e}")))?;

    let role = match role {
        Some(r) => r,
        None => {
            // Platform admins (JWT role claim) can access any workspace
            // even without explicit membership. This enables cross-workspace
            // management and support workflows.
            if claims.role == "admin" {
                "admin".to_string()
            } else {
                return Err(AppError::forbidden(
                    "You are not a member of this workspace",
                ));
            }
        }
    };

    let ws_ctx = WorkspaceContext {
        workspace_id,
        workspace_role: WorkspaceRole::from_str(&role),
    };

    req.extensions_mut().insert(ws_ctx);

    // Run the handler within the workspace task-local scope.
    // Sets both PG RLS (WORKSPACE_ID) and graph isolation (GRAPH_WORKSPACE_ID)
    // so all queries — relational and graph — are automatically workspace-scoped.
    let response = WORKSPACE_ID
        .scope(
            workspace_id,
            GRAPH_WORKSPACE_ID.scope(workspace_id, next.run(req)),
        )
        .await;

    Ok(response)
}
