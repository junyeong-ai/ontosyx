use std::time::{Duration, Instant};

use axum::{
    extract::{Request, State},
    http::HeaderValue,
    middleware::Next,
    response::Response,
};
use dashmap::DashMap;
use jsonwebtoken::{Algorithm, DecodingKey, Validation};
use serde::{Deserialize, Serialize};
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
/// Tries auth methods in order:
///   1. JWT (cookie or Authorization header)
///   2. DB-backed API key (X-API-Key header → sha256 → `api_keys` table)
///
/// On successful JWT auth, injects `AuthClaims` into request extensions.
/// On successful API key auth, injects a synthetic `AuthClaims` whose
/// `sub` is `apikey:<label>` and whose `exp` is short (1h) so a downstream
/// claim cache cannot bypass DB revocation for long.
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

    // Try DB-backed API key
    if let Some(presented) = req.headers().get("x-api-key").and_then(|v| v.to_str().ok()) {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(presented.as_bytes());
        let hash = hasher.finalize().to_vec();

        // SYSTEM_BYPASS so the lookup works without a workspace context;
        // RLS would otherwise hide global keys (workspace_id IS NULL).
        let store = state.store.clone();
        let lookup = ox_store::SYSTEM_BYPASS
            .scope(true, async move { store.find_api_key_by_hash(&hash).await })
            .await;

        match lookup {
            Ok(Some(key)) => {
                let label = key.label.clone();
                // Short TTL: every request re-hits `find_api_key_by_hash`
                // so the long-`exp` claim doesn't matter today, but a
                // 1h cap means any future caller that caches `AuthClaims`
                // still respects DB revocation within an hour.
                //
                // `role` comes from the DB row (Phase 1 migration 0010).
                // The CHECK constraint already restricts the column to
                // `admin | designer | viewer`, so this copy is safe to
                // embed in the synthetic JWT claim without further
                // validation.
                let now = chrono::Utc::now().timestamp() as usize;
                let claims = AuthClaims {
                    sub: format!("apikey:{label}"),
                    email: format!("{label}@apikey.ontosyx.local"),
                    name: Some(format!("API Key: {label}")),
                    role: key.role.clone(),
                    iss: "ontosyx-api-key".to_string(),
                    iat: now,
                    exp: now + 3600,
                };
                req.extensions_mut().insert(claims);
                return Ok(next.run(req).await);
            }
            Ok(None) => {
                // Unknown key — fall through to the rejection branch
                // below so the caller sees a uniform 401.
            }
            Err(e) => {
                tracing::warn!(error = %e, "API key DB lookup failed");
            }
        }
    }

    // No auth method succeeded.
    if state.auth_config.jwt_secret.is_none() {
        tracing::error!(
            "Auth request rejected and JWT is disabled — server has no usable auth method. \
             Set OX_AUTH__JWT_SECRET, or seed an API key (OX_AUTH__BOOTSTRAP_KEY for first boot)."
        );
        return Err(AppError::service_unavailable(
            "Authentication not configured.",
        ));
    }
    Err(AppError::unauthorized(
        "Invalid or missing authentication. Provide a valid JWT or API key.",
    ))
}

// ---------------------------------------------------------------------------
// Helpers for extracting claims in handlers
// ---------------------------------------------------------------------------

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
        // `spawn_system` wraps the future in SYSTEM_BYPASS — the cleanup
        // sweep is in-memory only today, but adopting the shared helper
        // keeps us honest once the limiter grows a persisted audit log.
        crate::spawn_scoped::spawn_system(async move {
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
    use crate::principal::Principal;
    use crate::workspace::{WorkspaceContext, WorkspaceRole};
    use tracing::Instrument;

    let claims = req
        .extensions()
        .get::<AuthClaims>()
        .cloned()
        .ok_or_else(|| AppError::unauthorized("Authentication required"))?;

    // Machine principals (system tasks and API keys) resolve workspace
    // from the X-Workspace-Id header. They have admin-level access, so
    // any workspace is valid. No fallback — callers must explicitly
    // specify which workspace to operate on.
    if crate::principal::is_machine_sub(&claims.sub) {
        let workspace_id = req
            .headers()
            .get("x-workspace-id")
            .ok_or_else(|| {
                AppError::bad_request(
                    "X-Workspace-Id header required. Call GET /workspaces first to list available workspaces.",
                )
            })?
            .to_str()
            .map_err(|_| AppError::bad_request("Invalid X-Workspace-Id header"))?
            .parse::<Uuid>()
            .map_err(|_| AppError::bad_request("X-Workspace-Id must be a valid UUID"))?;

        let ws_ctx = WorkspaceContext {
            workspace_id,
            workspace_role: WorkspaceRole::Owner,
        };
        req.extensions_mut().insert(ws_ctx.clone());

        let principal = Principal::from_claims(&claims);
        let span = tracing::info_span!(
            "request",
            workspace_id = %workspace_id,
            principal = %claims.sub,
        );
        let response = scope_request(
            &state,
            &principal,
            &ws_ctx,
            workspace_id,
            next.run(req).instrument(span),
        )
        .await?;
        return Ok(response);
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
        workspace_role: WorkspaceRole::from_db_string(&role),
    };

    req.extensions_mut().insert(ws_ctx.clone());

    let principal = Principal::from_claims(&claims);
    let span = tracing::info_span!(
        "request",
        workspace_id = %workspace_id,
        user_id = %user_id,
    );
    scope_request(
        &state,
        &principal,
        &ws_ctx,
        workspace_id,
        next.run(req).instrument(span),
    )
    .await
}

/// Scope a request future under every per-request task-local the
/// downstream stack reads: PG RLS, graph isolation, ACL snapshot,
/// and the rewriter principal.
///
/// `WORKSPACE_ID` and `GRAPH_WORKSPACE_ID` are scoped first so that
/// the ACL snapshot lookup itself runs under RLS — the
/// `acl_policies` table's `ws_isolation` policy casts
/// `app.workspace_id::uuid`, which fails on the empty default if
/// the lookup runs outside the scope.
///
/// **Fail-closed.** A failure to load the ACL snapshot rejects the
/// request rather than proceeding with empty policies. A transient
/// DB hiccup surfaces as a 503 the operator can retry; an
/// authentication-store outage never produces silently-unauthorised
/// query traffic.
async fn scope_request<F, R>(
    state: &AppState,
    principal: &crate::principal::Principal,
    ws: &crate::workspace::WorkspaceContext,
    workspace_id: Uuid,
    fut: F,
) -> Result<R, AppError>
where
    F: std::future::Future<Output = R>,
{
    use ox_runtime::{GRAPH_ACL_SNAPSHOT, GRAPH_PRINCIPAL, GRAPH_WORKSPACE_ID};
    use ox_store::WORKSPACE_ID;

    let request_principal = crate::acl_enforcement::request_principal(principal, ws);
    let store = state.store.clone();
    let principal = principal.clone();
    let ws = ws.clone();

    WORKSPACE_ID
        .scope(workspace_id, async move {
            GRAPH_WORKSPACE_ID
                .scope(workspace_id, async move {
                    let snapshot = crate::acl_enforcement::load_acl_snapshot(
                        store.as_ref(),
                        &principal,
                        &ws,
                    )
                    .await?;
                    let result = GRAPH_ACL_SNAPSHOT
                        .scope(snapshot, async move {
                            match request_principal {
                                Some(p) => GRAPH_PRINCIPAL.scope(p, fut).await,
                                None => fut.await,
                            }
                        })
                        .await;
                    Ok::<R, AppError>(result)
                })
                .await
        })
        .await
}
