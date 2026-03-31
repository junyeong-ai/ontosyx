// OIDC provider config includes standard fields (client_secret, scopes)
// used by the Authorization Code Flow, not yet consumed in the ID Token flow.
#![allow(dead_code)]

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use jsonwebtoken::{Algorithm, DecodingKey, Validation};
use serde::Deserialize;
use tokio::sync::RwLock;

use crate::error::AppError;

// ---------------------------------------------------------------------------
// Generic OIDC Provider
// ---------------------------------------------------------------------------
// Supports any OpenID Connect provider via .well-known/openid-configuration
// discovery. Google, Microsoft, Okta, Auth0, Keycloak — all work with the
// same code. Adding a new provider requires only config (zero code changes).
// ---------------------------------------------------------------------------

/// Configuration for a single OIDC provider.
#[derive(Debug, Clone, Deserialize)]
pub struct OidcProviderConfig {
    /// Display name: "google", "microsoft", "okta", etc.
    pub name: String,
    /// OIDC issuer URL (e.g., "https://accounts.google.com")
    /// Used for .well-known/openid-configuration discovery.
    pub issuer_url: String,
    /// OAuth2 client ID
    pub client_id: String,
    /// OAuth2 client secret (optional for public clients / implicit flow)
    pub client_secret: Option<String>,
    /// Required scopes (default: ["openid", "email", "profile"])
    #[serde(default = "default_scopes")]
    pub scopes: Vec<String>,
}

fn default_scopes() -> Vec<String> {
    vec![
        "openid".to_string(),
        "email".to_string(),
        "profile".to_string(),
    ]
}

/// User information extracted from a verified OIDC ID token.
#[derive(Debug, Clone)]
pub struct OidcUserInfo {
    /// Provider-specific subject identifier
    pub sub: String,
    pub email: Option<String>,
    pub email_verified: Option<bool>,
    pub name: Option<String>,
    pub picture: Option<String>,
}

// ---------------------------------------------------------------------------
// OIDC Discovery + JWKS caching
// ---------------------------------------------------------------------------

/// OpenID Connect discovery document.
#[derive(Debug, Deserialize)]
struct OidcDiscovery {
    issuer: String,
    jwks_uri: String,
    authorization_endpoint: Option<String>,
    token_endpoint: Option<String>,
}

/// JWKS response from the provider.
#[derive(Debug, Deserialize)]
struct JwksResponse {
    keys: Vec<JwkKey>,
}

#[derive(Debug, Deserialize)]
struct JwkKey {
    kid: String,
    kty: String,
    alg: Option<String>,
    // RSA components
    n: Option<String>,
    e: Option<String>,
}

/// Cached JWKS keys for a single provider.
struct JwksCache {
    keys: HashMap<String, DecodingKey>,
    fetched_at: Instant,
}

const JWKS_CACHE_TTL: Duration = Duration::from_secs(3600);

/// A configured OIDC provider with cached JWKS.
pub struct OidcProvider {
    config: OidcProviderConfig,
    jwks_uri: String,
    issuer: String,
    cache: Arc<RwLock<Option<JwksCache>>>,
}

impl OidcProvider {
    /// Create an OIDC provider by discovering endpoints from the issuer URL.
    pub async fn from_discovery(config: OidcProviderConfig) -> Result<Self, AppError> {
        let discovery_url = format!(
            "{}/.well-known/openid-configuration",
            config.issuer_url.trim_end_matches('/')
        );

        let discovery: OidcDiscovery = reqwest::get(&discovery_url)
            .await
            .map_err(|e| {
                AppError::internal(format!("OIDC discovery failed for '{}': {e}", config.name))
            })?
            .json()
            .await
            .map_err(|e| {
                AppError::internal(format!(
                    "OIDC discovery parse failed for '{}': {e}",
                    config.name
                ))
            })?;

        tracing::info!(
            provider = %config.name,
            issuer = %discovery.issuer,
            jwks_uri = %discovery.jwks_uri,
            "OIDC provider discovered"
        );

        Ok(Self {
            config,
            jwks_uri: discovery.jwks_uri,
            issuer: discovery.issuer,
            cache: Arc::new(RwLock::new(None)),
        })
    }

    /// Verify an ID token and return user info.
    pub async fn verify_token(&self, id_token: &str) -> Result<OidcUserInfo, AppError> {
        // Extract kid from JWT header
        let header = jsonwebtoken::decode_header(id_token).map_err(|e| {
            tracing::debug!(provider = %self.config.name, error = %e, "Failed to decode token header");
            AppError::unauthorized("Invalid ID token")
        })?;

        let kid = header
            .kid
            .ok_or_else(|| AppError::unauthorized("ID token missing key ID (kid)"))?;

        // Get JWKS keys (with cache)
        let mut keys = self.get_jwks_keys().await?;

        // If kid not found, force refresh (key rotation)
        if !keys.contains_key(&kid) {
            tracing::info!(
                provider = %self.config.name,
                kid = %kid,
                "Key ID not in cache, refreshing JWKS"
            );
            self.invalidate_cache().await;
            keys = self.get_jwks_keys().await?;
        }

        let decoding_key = keys.get(&kid).ok_or_else(|| {
            tracing::warn!(
                provider = %self.config.name,
                kid = %kid,
                "Key ID not found after JWKS refresh"
            );
            AppError::unauthorized("Signing key not found")
        })?;

        // Validate signature and claims
        let mut validation = Validation::new(Algorithm::RS256);
        validation.validate_exp = true;
        validation.set_issuer(&[&self.issuer]);
        validation.set_audience(&[&self.config.client_id]);

        let token_data = jsonwebtoken::decode::<OidcClaims>(id_token, decoding_key, &validation)
            .map_err(|e| {
                tracing::debug!(
                    provider = %self.config.name,
                    error = %e,
                    "ID token validation failed"
                );
                AppError::unauthorized("Invalid or expired ID token")
            })?;

        let claims = token_data.claims;

        // Require email
        if claims.email.is_none() {
            return Err(AppError::unauthorized("Token missing email claim"));
        }
        if !claims.email_verified.unwrap_or(false) {
            return Err(AppError::unauthorized("Email not verified"));
        }

        Ok(OidcUserInfo {
            sub: claims.sub,
            email: claims.email,
            email_verified: claims.email_verified,
            name: claims.name,
            picture: claims.picture,
        })
    }

    /// Provider name for matching.
    pub fn name(&self) -> &str {
        &self.config.name
    }

    // ---- Internal ----

    async fn get_jwks_keys(&self) -> Result<HashMap<String, DecodingKey>, AppError> {
        // Fast path: cached
        {
            let guard = self.cache.read().await;
            if let Some(ref cached) = *guard
                && cached.fetched_at.elapsed() < JWKS_CACHE_TTL
            {
                return Ok(cached.keys.clone());
            }
        }

        // Slow path: fetch under write lock
        let mut guard = self.cache.write().await;
        if let Some(ref cached) = *guard
            && cached.fetched_at.elapsed() < JWKS_CACHE_TTL
        {
            return Ok(cached.keys.clone());
        }

        let keys = self.fetch_jwks().await?;
        *guard = Some(JwksCache {
            keys: keys.clone(),
            fetched_at: Instant::now(),
        });
        Ok(keys)
    }

    async fn fetch_jwks(&self) -> Result<HashMap<String, DecodingKey>, AppError> {
        let resp = reqwest::get(&self.jwks_uri).await.map_err(|e| {
            AppError::internal(format!(
                "Failed to fetch JWKS for '{}': {e}",
                self.config.name
            ))
        })?;

        let jwks: JwksResponse = resp.json().await.map_err(|e| {
            AppError::internal(format!(
                "Failed to parse JWKS for '{}': {e}",
                self.config.name
            ))
        })?;

        let mut keys = HashMap::new();
        for key in jwks.keys {
            if key.kty != "RSA" {
                continue;
            }
            let (Some(n), Some(e)) = (&key.n, &key.e) else {
                continue;
            };
            match DecodingKey::from_rsa_components(n, e) {
                Ok(dk) => {
                    keys.insert(key.kid, dk);
                }
                Err(err) => {
                    tracing::warn!(
                        provider = %self.config.name,
                        kid = %key.kid,
                        error = %err,
                        "Skipping invalid JWKS key"
                    );
                }
            }
        }

        if keys.is_empty() {
            return Err(AppError::internal(format!(
                "No valid RSA keys in JWKS for '{}'",
                self.config.name
            )));
        }

        tracing::info!(
            provider = %self.config.name,
            key_count = keys.len(),
            "JWKS keys loaded"
        );
        Ok(keys)
    }

    async fn invalidate_cache(&self) {
        let mut guard = self.cache.write().await;
        *guard = None;
    }
}

/// Standard OIDC ID token claims.
#[derive(Debug, Deserialize)]
struct OidcClaims {
    sub: String,
    email: Option<String>,
    email_verified: Option<bool>,
    name: Option<String>,
    picture: Option<String>,
}

// ---------------------------------------------------------------------------
// OidcProviderRegistry — manages all configured providers
// ---------------------------------------------------------------------------

/// Registry of configured OIDC providers, keyed by name.
pub struct OidcProviderRegistry {
    providers: HashMap<String, OidcProvider>,
}

impl OidcProviderRegistry {
    /// Initialize all providers from config via discovery.
    /// Providers that fail discovery are logged and skipped.
    pub async fn from_configs(configs: Vec<OidcProviderConfig>) -> Self {
        let mut providers = HashMap::new();
        for config in configs {
            let name = config.name.clone();
            match OidcProvider::from_discovery(config).await {
                Ok(provider) => {
                    providers.insert(name, provider);
                }
                Err(e) => {
                    tracing::error!(
                        provider = %name,
                        error = ?e,
                        "Failed to initialize OIDC provider — skipping"
                    );
                }
            }
        }
        Self { providers }
    }

    /// Create an empty registry (no OIDC providers configured).
    pub fn empty() -> Self {
        Self {
            providers: HashMap::new(),
        }
    }

    /// Get a provider by name.
    pub fn get(&self, name: &str) -> Option<&OidcProvider> {
        self.providers.get(name)
    }

    /// List all configured provider names.
    pub fn provider_names(&self) -> Vec<&str> {
        self.providers.keys().map(|s| s.as_str()).collect()
    }

    /// Check if any providers are configured.
    pub fn is_empty(&self) -> bool {
        self.providers.is_empty()
    }
}
