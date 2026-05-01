use axum::Json;
use axum::extract::{Extension, State};
use chrono::{DateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use ox_store::User;

use crate::error::AppError;
use crate::middleware::{AuthClaims, create_jwt};
use crate::principal::Principal;
use crate::response::ApiResponse;
use crate::state::AppState;

// ---------------------------------------------------------------------------
// POST /auth/token — exchange OIDC info for platform JWT
// ---------------------------------------------------------------------------

#[derive(Deserialize, utoipa::ToSchema)]
pub struct CreateAuthTokenRequest {
    /// The ID token from an OIDC provider
    pub id_token: String,
    /// OIDC provider name (e.g., "google", "microsoft", "okta")
    pub provider: String,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct CreateAuthTokenResponse {
    pub token: String,
    pub user: UserInfo,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct UserInfo {
    pub id: Uuid,
    pub email: String,
    pub name: Option<String>,
    pub picture: Option<String>,
    pub role: String,
}

#[utoipa::path(
    post,
    path = "/auth/token",
    request_body = CreateAuthTokenRequest,
    responses(
        (status = 200, description = "Token created", body = CreateAuthTokenResponse),
        (status = 401, description = "Invalid ID token", body = inline(crate::openapi::ErrorResponse)),
    ),
    tag = "Auth",
)]
pub(crate) async fn create_token(
    State(state): State<AppState>,
    Json(req): Json<CreateAuthTokenRequest>,
) -> Result<Json<ApiResponse<CreateAuthTokenResponse>>, AppError> {
    let jwt_secret = state
        .auth_config
        .jwt_secret
        .as_ref()
        .ok_or_else(|| AppError::service_unavailable("JWT authentication not configured"))?;

    // Look up the OIDC provider
    let provider = state.oidc_providers.get(&req.provider).ok_or_else(|| {
        let available = state.oidc_providers.provider_names();
        AppError::bad_request(format!(
            "Unknown provider '{}'. Available: {:?}",
            req.provider, available
        ))
    })?;

    // Verify the ID token via generic OIDC (RS256 + JWKS + claims validation)
    let oidc_user = provider.verify_token(&req.id_token).await?;

    let email = oidc_user
        .email
        .ok_or_else(|| AppError::unauthorized("Token missing email"))?;
    let now = Utc::now();

    // Upsert user in DB
    let user = User {
        id: Uuid::new_v4(),
        email: email.clone(),
        name: oidc_user.name.clone(),
        picture: oidc_user.picture.clone(),
        provider: req.provider.clone(),
        provider_sub: oidc_user.sub,
        role: "designer".to_string(),
        token_version: 0,
        created_at: now,
        last_login_at: Some(now),
    };

    // Auth runs before workspace_context middleware (the route is
    // public), so user / membership writes lack a per-request
    // workspace scope. Wrap the user-side bookkeeping in
    // `SYSTEM_BYPASS` — login is a system-level operation by design,
    // and the bypass keeps the `require_workspace_context` guard
    // happy on every mutating call below.
    let user = ox_store::SYSTEM_BYPASS
        .scope(true, async {
            let mut user = state
                .store
                .upsert_user(&user)
                .await
                .map_err(AppError::from)?;

            // Auto-promote first user to admin
            let user_count = state
                .store
                .count_users()
                .await
                .map_err(AppError::from)?;
            if user_count == 1 && user.role != "admin" {
                let should_promote = match &state.auth_config.first_admin_email {
                    Some(admin_email) => user.email == *admin_email,
                    None => true,
                };
                if should_promote {
                    state
                        .store
                        .update_user_role(user.id, "admin")
                        .await
                        .map_err(AppError::from)?;
                    user.role = "admin".to_string();
                    tracing::info!(user_id = %user.id, "First user auto-promoted to admin");
                }
            }

            // Auto-join default workspace for new users
            if user.created_at == now
                && let Ok(Some(ws)) = state
                    .store
                    .get_workspace_by_slug(crate::workspace::DEFAULT_WORKSPACE_SLUG)
                    .await
                && let Err(e) = state
                    .store
                    .add_workspace_member(ws.id, user.id, "member")
                    .await
            {
                tracing::error!(
                    user_id = %user.id,
                    workspace_id = %ws.id,
                    error = ?e,
                    "Failed to auto-join default workspace"
                );
            }

            Ok::<User, AppError>(user)
        })
        .await?;

    // Create platform JWT
    let exp_secs = state.auth_config.session_hours * 3600;
    let iat = now.timestamp() as usize;
    let exp = iat + exp_secs as usize;

    let claims = AuthClaims {
        sub: user.id.to_string(),
        email: user.email.clone(),
        name: user.name.clone(),
        role: user.role.clone(),
        iss: "ontosyx".to_string(),
        exp,
        iat,
        // Every issued platform JWT is keyed by a unique `jti` for
        // per-token revocation, plus a `tv` snapshot of the user's
        // bulk-invalidation counter — both axes feed
        // `require_auth`'s revocation check.
        jti: Uuid::new_v4(),
        tv: user.token_version,
    };

    let token = create_jwt(&claims, jwt_secret)?;

    tracing::info!(
        user_id = %user.id,
        email = %user.email,
        provider = %req.provider,
        "User authenticated via OIDC"
    );

    Ok(ApiResponse::of(CreateAuthTokenResponse {
        token,
        user: UserInfo {
            id: user.id,
            email: user.email,
            name: user.name,
            picture: user.picture,
            role: user.role,
        },
    }))
}

// ---------------------------------------------------------------------------
// GET /auth/me — return current user info from JWT
// ---------------------------------------------------------------------------

#[derive(Serialize, utoipa::ToSchema)]
pub struct AuthMeResponse {
    pub user: UserInfo,
}

#[utoipa::path(
    get,
    path = "/auth/me",
    responses(
        (status = 200, description = "Current user info", body = AuthMeResponse),
        (status = 401, description = "Not authenticated", body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("bearer" = [])),
    tag = "Auth",
)]
pub(crate) async fn me(
    State(state): State<AppState>,
    principal: Principal,
) -> Result<Json<ApiResponse<AuthMeResponse>>, AppError> {
    // Machine principals (system tasks + API keys) don't have a real
    // DB user row; return a synthetic response so clients can still call
    // `/auth/me` to confirm the key works.
    if principal.is_machine() {
        return Ok(ApiResponse::of(AuthMeResponse {
            user: UserInfo {
                id: Uuid::nil(),
                email: principal.email,
                name: Some("API Key".to_string()),
                picture: None,
                role: principal.role.as_str().to_string(),
            },
        }));
    }

    let user_id =
        Uuid::parse_str(&principal.id).map_err(|_| AppError::unauthorized("Invalid user ID"))?;

    let user = state
        .store
        .get_user_by_id(user_id)
        .await
        .map_err(AppError::from)?
        .ok_or_else(|| AppError::not_found("User"))?;

    Ok(ApiResponse::of(AuthMeResponse {
        user: UserInfo {
            id: user.id,
            email: user.email,
            name: user.name,
            picture: user.picture,
            role: user.role,
        },
    }))
}

// ---------------------------------------------------------------------------
// POST /auth/logout — revoke the caller's current JWT
// ---------------------------------------------------------------------------

#[derive(Serialize, utoipa::ToSchema)]
pub struct LogoutResponse {
    /// Always `true` on success — the response shape mirrors other
    /// auth endpoints so the BFF can branch on JSON instead of HTTP
    /// status alone.
    pub revoked: bool,
}

/// Revoke the caller's current platform JWT so it can no longer be
/// presented as proof of identity. Inserts a row in `revoked_jwts`
/// keyed by the token's `jti`, and drops the cached negative-result
/// in [`JwtRevocationCache`] so the next request from any holder of
/// the same token sees the revocation immediately.
///
/// Idempotent — repeated calls are safe and return the same shape.
/// API-key principals short-circuit: API keys don't have a JWT
/// surface to revoke, so the endpoint surfaces a `400` so the client
/// can route them to the admin "delete API key" path instead.
#[utoipa::path(
    post,
    path = "/auth/logout",
    responses(
        (status = 200, description = "JWT revoked", body = LogoutResponse),
        (status = 400, description = "API key principals cannot self-logout", body = inline(crate::openapi::ErrorResponse)),
        (status = 401, description = "Not authenticated", body = inline(crate::openapi::ErrorResponse)),
    ),
    security(("bearer" = [])),
    tag = "Auth",
)]
pub(crate) async fn logout(
    State(state): State<AppState>,
    Extension(claims): Extension<AuthClaims>,
) -> Result<Json<ApiResponse<LogoutResponse>>, AppError> {
    if claims.is_api_key() {
        return Err(AppError::bad_request(
            "API key principals cannot self-logout. Delete the key via \
             the admin endpoint to revoke access.",
        ));
    }

    // The original JWT's `exp` is the natural truncation point — once
    // it has passed, the token is unusable regardless of revocation
    // state and the row can be reaped by the cleanup cron.
    let expires_at = jwt_exp_to_datetime(claims.exp).ok_or_else(|| {
        AppError::unauthorized("Token has no usable expiry — cannot revoke")
    })?;
    let user_id = claims.user_id().ok();
    let jti = claims.jti;

    let store = state.store.clone();
    ox_store::SYSTEM_BYPASS
        .scope(true, async move {
            store
                .revoke_jwt(jti, expires_at, user_id, Some("user logout".to_string()))
                .await
        })
        .await
        .map_err(AppError::from)?;

    // Drop the (likely cached) negative result so the next request
    // from any holder of the same token sees the revocation without
    // waiting out the TTL.
    state.jwt_revocation_cache.invalidate_jti(jti);

    Ok(ApiResponse::of(LogoutResponse { revoked: true }))
}

fn jwt_exp_to_datetime(exp: usize) -> Option<DateTime<Utc>> {
    let secs: i64 = exp.try_into().ok()?;
    Utc.timestamp_opt(secs, 0).single()
}
