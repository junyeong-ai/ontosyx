use axum::Json;
use axum::extract::State;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use ox_store::User;

use crate::error::AppError;
use crate::middleware::{AuthClaims, create_jwt};
use crate::principal::Principal;
use crate::state::AppState;

// ---------------------------------------------------------------------------
// POST /auth/token — exchange OIDC info for platform JWT
// ---------------------------------------------------------------------------

#[derive(Deserialize, utoipa::ToSchema)]
pub struct AuthTokenCreateRequest {
    /// The ID token from an OIDC provider
    pub id_token: String,
    /// OIDC provider name (e.g., "google", "microsoft", "okta")
    pub provider: String,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct AuthTokenResponse {
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
    request_body = AuthTokenCreateRequest,
    responses(
        (status = 200, description = "Token created", body = AuthTokenResponse),
        (status = 401, description = "Invalid ID token", body = inline(crate::openapi::ErrorResponse)),
    ),
    tag = "Auth",
)]
pub async fn create_token(
    State(state): State<AppState>,
    Json(req): Json<AuthTokenCreateRequest>,
) -> Result<Json<AuthTokenResponse>, AppError> {
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
        created_at: now,
        last_login_at: Some(now),
    };

    let mut user = state
        .store
        .upsert_user(&user)
        .await
        .map_err(AppError::from)?;

    // Auto-promote first user to admin
    let user_count = state.store.get_user_count().await.map_err(AppError::from)?;
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
    };

    let token = create_jwt(&claims, jwt_secret)?;

    tracing::info!(
        user_id = %user.id,
        email = %user.email,
        provider = %req.provider,
        "User authenticated via OIDC"
    );

    Ok(Json(AuthTokenResponse {
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
pub async fn me(
    State(state): State<AppState>,
    principal: Principal,
) -> Result<Json<AuthMeResponse>, AppError> {
    // For API key access, return the synthetic system principal
    if principal.id.starts_with("system:") {
        return Ok(Json(AuthMeResponse {
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

    Ok(Json(AuthMeResponse {
        user: UserInfo {
            id: user.id,
            email: user.email,
            name: user.name,
            picture: user.picture,
            role: user.role,
        },
    }))
}
