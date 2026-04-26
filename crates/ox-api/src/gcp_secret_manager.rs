//! GCP Secret Manager resolver for the `gcp-sm:` scheme.
//!
//! Compiled in only when the `gcp-sm` cargo feature is on. The
//! resolver dereferences `gcp-sm:` references against the live
//! Secret Manager REST API, reusing yup-oauth2's ADC chain (the
//! same crate `gcp-bigquery-client` uses internally — a single token
//! source supports both BigQuery and Secret Manager on this server).
//!
//! ## Reference forms
//!
//! - **Long form** (canonical):
//!   `gcp-sm:projects/{project}/secrets/{secret}/versions/{version}`
//!   maps directly onto the
//!   [`SecretManagerService.AccessSecretVersion`](https://cloud.google.com/secret-manager/docs/reference/rest/v1/projects.secrets.versions/access)
//!   resource path. `version` may be a numeric id or `latest`.
//! - **Short form** (convenience):
//!   `gcp-sm:{secret_id}` expands to
//!   `projects/{ADC_DEFAULT_PROJECT}/secrets/{secret_id}/versions/latest`.
//!   ADC's default project comes from `GOOGLE_CLOUD_PROJECT` /
//!   `GCLOUD_PROJECT` / `gcloud config get-value project` (the
//!   environment-variable subset, since the resolver runs in-process).
//!
//! ## Caching
//!
//! Off by default — secret rotation surfaces immediately on the next
//! request. Set `OXY_GCP_SM_CACHE_TTL=<seconds>` (e.g. `300`) to
//! enable a per-resource memo with the given TTL. Cache state is
//! held inside a single resolver instance; restart drops it.
//!
//! ## Error policy
//!
//! Authentication / network / Secret Manager-side failures surface
//! as `AppError::bad_request` with the failing reference and a
//! short, descriptive cause. We **do not retry** — yup-oauth2 itself
//! retries the underlying token endpoint, and the surrounding
//! credential dispatch should treat a hard failure as a hard
//! failure. Permission denied (403) and not-found (404) are
//! distinguished in the message so an operator can tell them apart.

#![cfg(feature = "gcp-sm")]

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use base64::Engine as _;
use serde::Deserialize;
use tokio::sync::Mutex;
use hyper_util::client::legacy::connect::HttpConnector;
use yup_oauth2::authenticator::{ApplicationDefaultCredentialsTypes, Authenticator};
use yup_oauth2::hyper_rustls::HttpsConnector;
use yup_oauth2::{
    ApplicationDefaultCredentialsAuthenticator, ApplicationDefaultCredentialsFlowOpts,
    AuthorizedUserAuthenticator, ServiceAccountAuthenticator,
};

use crate::credential::SecretResolver;
use crate::error::AppError;

const SCOPE: &str = "https://www.googleapis.com/auth/cloud-platform";
const SECRET_MANAGER_ENDPOINT: &str = "https://secretmanager.googleapis.com/v1";
const CACHE_TTL_ENV: &str = "OXY_GCP_SM_CACHE_TTL";
const PROJECT_ENV_KEYS: &[&str] = &["GOOGLE_CLOUD_PROJECT", "GCLOUD_PROJECT"];

/// Resolver for the `gcp-sm:` scheme. One instance per server.
pub struct GcpSecretManagerResolver {
    auth: Authenticator<HttpsConnector<HttpConnector>>,
    http: reqwest::Client,
    /// Optional default project used to expand the short-form
    /// `gcp-sm:{secret_id}` reference. `None` means short form is
    /// rejected — the operator must use the long form.
    default_project: Option<String>,
    /// `Some(ttl)` enables a per-resource memo for `ttl` after fetch.
    /// `None` disables caching entirely (default).
    cache: Option<CacheState>,
}

struct CacheState {
    ttl: Duration,
    entries: Mutex<HashMap<String, CacheEntry>>,
}

struct CacheEntry {
    value: Arc<str>,
    expires_at: Instant,
}

impl GcpSecretManagerResolver {
    /// Build a resolver from Application Default Credentials. Order:
    ///
    /// 1. `GOOGLE_APPLICATION_CREDENTIALS` env var → service account
    ///    JSON file.
    /// 2. `~/.config/gcloud/application_default_credentials.json` →
    ///    either authorized-user (`gcloud auth application-default
    ///    login`) or service-account JSON, depending on the file's
    ///    `type` field.
    /// 3. GCE / GKE workload-identity metadata server.
    ///
    /// Default project for the short form is read from
    /// `GOOGLE_CLOUD_PROJECT` / `GCLOUD_PROJECT` (env-var subset of
    /// gcloud's project resolution).
    pub async fn from_application_default_credentials() -> Result<Self, AppError> {
        let auth = build_adc_authenticator().await?;
        let cache = read_cache_ttl_from_env()?.map(|ttl| CacheState {
            ttl,
            entries: Mutex::new(HashMap::new()),
        });
        let default_project = PROJECT_ENV_KEYS
            .iter()
            .find_map(|k| std::env::var(k).ok().filter(|v| !v.is_empty()));
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .build()
            .map_err(|e| AppError::bad_request(format!(
                "failed to build HTTP client for GCP Secret Manager: {e}"
            )))?;
        Ok(Self {
            auth,
            http,
            default_project,
            cache,
        })
    }
}

#[async_trait]
impl SecretResolver for GcpSecretManagerResolver {
    async fn resolve(&self, reference: &str) -> Result<Arc<str>, AppError> {
        let resource = parse_gcp_sm_reference(reference, self.default_project.as_deref())?;

        // Cache hit — return the memoised Arc, no network round-trip.
        if let Some(state) = &self.cache {
            let now = Instant::now();
            let mut guard = state.entries.lock().await;
            if let Some(entry) = guard.get(&resource)
                && entry.expires_at > now
            {
                return Ok(Arc::clone(&entry.value));
            }
            guard.retain(|_, e| e.expires_at > now);
        }

        let token = self
            .auth
            .token(&[SCOPE])
            .await
            .map_err(|e| AppError::bad_request(format!(
                "secret_ref 'gcp-sm:{resource}' — failed to mint ADC token: {e}"
            )))?;
        let bearer = token.token().ok_or_else(|| {
            AppError::bad_request(format!(
                "secret_ref 'gcp-sm:{resource}' — ADC returned an empty access token"
            ))
        })?;

        let url = format!("{SECRET_MANAGER_ENDPOINT}/{resource}:access");
        let resp = self
            .http
            .get(&url)
            .bearer_auth(bearer)
            .send()
            .await
            .map_err(|e| AppError::bad_request(format!(
                "secret_ref 'gcp-sm:{resource}' — Secret Manager request failed: {e}"
            )))?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(secret_manager_status_error(&resource, status.as_u16(), &body));
        }

        #[derive(Deserialize)]
        struct AccessResponse {
            payload: AccessPayload,
        }
        #[derive(Deserialize)]
        struct AccessPayload {
            data: String,
        }

        let body: AccessResponse = resp.json().await.map_err(|e| {
            AppError::bad_request(format!(
                "secret_ref 'gcp-sm:{resource}' — malformed Secret Manager response: {e}"
            ))
        })?;
        let raw = base64::engine::general_purpose::STANDARD
            .decode(body.payload.data.as_bytes())
            .map_err(|e| {
                AppError::bad_request(format!(
                    "secret_ref 'gcp-sm:{resource}' — base64 decode failed: {e}"
                ))
            })?;
        let text = String::from_utf8(raw).map_err(|e| {
            AppError::bad_request(format!(
                "secret_ref 'gcp-sm:{resource}' — payload is not valid UTF-8: {e}"
            ))
        })?;
        let value: Arc<str> = Arc::from(text);

        if let Some(state) = &self.cache {
            let mut guard = state.entries.lock().await;
            guard.insert(
                resource,
                CacheEntry {
                    value: Arc::clone(&value),
                    expires_at: Instant::now() + state.ttl,
                },
            );
        }
        Ok(value)
    }
}

// ---------------------------------------------------------------------------
// ADC authenticator dispatch
//
// yup-oauth2's `ApplicationDefaultCredentialsAuthenticator::builder`
// only handles two of Google's three ADC sources: a service-account
// JSON pointed at by `GOOGLE_APPLICATION_CREDENTIALS`, and the GCE /
// GKE metadata server. The third — the authorized-user JSON written
// by `gcloud auth application-default login` to
// `$HOME/.config/gcloud/application_default_credentials.json` — has
// to be wired manually below. Without that branch, `gcp-sm` only
// works on workload-identity / SA-key deployments and silently
// fails on a developer laptop.
// ---------------------------------------------------------------------------

async fn build_adc_authenticator(
) -> Result<Authenticator<HttpsConnector<HttpConnector>>, AppError> {
    if let Ok(path) = std::env::var("GOOGLE_APPLICATION_CREDENTIALS")
        && !path.is_empty()
    {
        let key = yup_oauth2::read_service_account_key(&path).await.map_err(|e| {
            AppError::bad_request(format!(
                "GOOGLE_APPLICATION_CREDENTIALS='{path}' — failed to read service account key: {e}"
            ))
        })?;
        return ServiceAccountAuthenticator::builder(key).build().await.map_err(|e| {
            AppError::bad_request(format!(
                "GOOGLE_APPLICATION_CREDENTIALS='{path}' — failed to build authenticator: {e}"
            ))
        });
    }

    if let Some(adc_path) = gcloud_adc_path()
        && tokio::fs::try_exists(&adc_path).await.unwrap_or(false)
    {
        // gcloud writes either {"type": "authorized_user", ...} or
        // {"type": "service_account", ...}. Try authorized-user
        // first (the common local-dev case), fall back to SA.
        if let Ok(secret) = yup_oauth2::read_authorized_user_secret(&adc_path).await {
            return AuthorizedUserAuthenticator::builder(secret)
                .build()
                .await
                .map_err(|e| AppError::bad_request(format!(
                    "{} — failed to build authorized-user authenticator: {e}",
                    adc_path.display()
                )));
        }
        if let Ok(key) = yup_oauth2::read_service_account_key(&adc_path).await {
            return ServiceAccountAuthenticator::builder(key)
                .build()
                .await
                .map_err(|e| AppError::bad_request(format!(
                    "{} — failed to build service-account authenticator: {e}",
                    adc_path.display()
                )));
        }
    }

    let opts = ApplicationDefaultCredentialsFlowOpts::default();
    match ApplicationDefaultCredentialsAuthenticator::builder(opts).await {
        ApplicationDefaultCredentialsTypes::InstanceMetadata(builder) => {
            builder.build().await.map_err(|e| AppError::bad_request(format!(
                "GCP Secret Manager: failed to build instance-metadata authenticator \
                 (workload identity not available?): {e}"
            )))
        }
        ApplicationDefaultCredentialsTypes::ServiceAccount(builder) => {
            builder.build().await.map_err(|e| AppError::bad_request(format!(
                "GCP Secret Manager: failed to build service-account authenticator: {e}"
            )))
        }
    }
}

fn gcloud_adc_path() -> Option<PathBuf> {
    let home = std::env::var("HOME").ok().filter(|s| !s.is_empty())?;
    Some(PathBuf::from(home).join(".config/gcloud/application_default_credentials.json"))
}

fn read_cache_ttl_from_env() -> Result<Option<Duration>, AppError> {
    match std::env::var(CACHE_TTL_ENV) {
        Ok(raw) => parse_cache_ttl(&raw),
        Err(_) => Ok(None),
    }
}

/// Pure parse: TTL string → optional `Duration`. Empty / `"0"` ⇒ off.
/// Split out from [`read_cache_ttl_from_env`] so the parsing rules can
/// be tested without mutating process-global env (the env-mutation
/// tests would race against each other in a parallel test runner).
fn parse_cache_ttl(raw: &str) -> Result<Option<Duration>, AppError> {
    if raw.is_empty() {
        return Ok(None);
    }
    let secs: u64 = raw.parse().map_err(|_| {
        AppError::bad_request(format!(
            "{CACHE_TTL_ENV}='{raw}' is not a valid u64 — set to a number of seconds, \
             or unset to disable caching"
        ))
    })?;
    if secs == 0 {
        return Ok(None);
    }
    Ok(Some(Duration::from_secs(secs)))
}

fn secret_manager_status_error(resource: &str, code: u16, body: &str) -> AppError {
    let trimmed = body.trim();
    match code {
        401 => AppError::bad_request(format!(
            "secret_ref 'gcp-sm:{resource}' — authentication rejected (401). \
             Verify ADC is valid (`gcloud auth application-default print-access-token` \
             on a developer machine, or workload-identity binding on GKE)."
        )),
        403 => AppError::bad_request(format!(
            "secret_ref 'gcp-sm:{resource}' — permission denied (403). \
             The ADC principal must have role roles/secretmanager.secretAccessor \
             on the secret or its enclosing project."
        )),
        404 => AppError::bad_request(format!(
            "secret_ref 'gcp-sm:{resource}' — secret or version not found (404). \
             Check the project / secret-id / version triple."
        )),
        _ => AppError::bad_request(format!(
            "secret_ref 'gcp-sm:{resource}' — Secret Manager returned HTTP {code}. \
             Body: {trimmed}"
        )),
    }
}

// ---------------------------------------------------------------------------
// Reference parsing
// ---------------------------------------------------------------------------

/// Parse a `gcp-sm:` reference and return the canonical resource
/// path the REST API expects (`projects/.../secrets/.../versions/...`,
/// no leading slash, no scheme). Pure string operation — no I/O.
fn parse_gcp_sm_reference(
    reference: &str,
    default_project: Option<&str>,
) -> Result<String, AppError> {
    let body = reference.strip_prefix("gcp-sm:").ok_or_else(|| {
        AppError::bad_request(format!(
            "secret_ref '{reference}' — gcp-sm scheme expected"
        ))
    })?;
    if body.is_empty() {
        return Err(AppError::bad_request(
            "secret_ref 'gcp-sm:' missing the secret reference after the colon",
        ));
    }
    // A `/` anywhere in the body is a strong signal the operator
    // meant the long form. Route everything `/`-bearing through the
    // long-form check so a typo like `gcp-sm:my/secret` surfaces as
    // "invalid resource path" (the helpful diagnostic) rather than
    // falling through to short-form validation and reporting
    // "invalid secret id" (which only describes the symptom).
    if body.contains('/') {
        let parts: Vec<&str> = body.split('/').collect();
        if parts.len() != 6
            || parts[0] != "projects"
            || parts[2] != "secrets"
            || parts[4] != "versions"
            || parts[1].is_empty()
            || parts[3].is_empty()
            || parts[5].is_empty()
        {
            return Err(AppError::bad_request(format!(
                "secret_ref 'gcp-sm:{body}' — invalid resource path. \
                 Expected projects/PROJECT/secrets/SECRET/versions/VERSION"
            )));
        }
        return Ok(body.to_string());
    }
    // Short form is opt-in: only accepted when the resolver has a
    // default project. The error message names the env var the
    // operator can set, instead of silently rejecting.
    let project = default_project.ok_or_else(|| {
        AppError::bad_request(format!(
            "secret_ref 'gcp-sm:{body}' — short form requires GOOGLE_CLOUD_PROJECT \
             (or GCLOUD_PROJECT) to be set. Use the long form \
             projects/.../secrets/{body}/versions/latest if no default project \
             is available."
        ))
    })?;
    if !is_valid_secret_id(body) {
        return Err(AppError::bad_request(format!(
            "secret_ref 'gcp-sm:{body}' — invalid secret id. \
             Allowed: alphanumerics, `_` and `-`, length 1..=255."
        )));
    }
    Ok(format!("projects/{project}/secrets/{body}/versions/latest"))
}

fn is_valid_secret_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 255
        && s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
}

// ---------------------------------------------------------------------------
// Tests — string-only paths. Live ADC + Secret Manager round-trips
// are out of scope here; they require a real GCP project and are
// covered by the BigQuery integration test pattern (Phase 1) for
// the same auth path.
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn parse_long_form_round_trips() {
        let r = "gcp-sm:projects/my-proj/secrets/db_password/versions/3";
        let resource = parse_gcp_sm_reference(r, None).unwrap();
        assert_eq!(resource, "projects/my-proj/secrets/db_password/versions/3");
    }

    #[test]
    fn parse_long_form_accepts_latest_keyword() {
        let r = "gcp-sm:projects/p/secrets/s/versions/latest";
        let resource = parse_gcp_sm_reference(r, None).unwrap();
        assert_eq!(resource, "projects/p/secrets/s/versions/latest");
    }

    #[test]
    fn parse_short_form_expands_with_default_project() {
        let r = "gcp-sm:db-password";
        let resource = parse_gcp_sm_reference(r, Some("my-proj")).unwrap();
        assert_eq!(
            resource,
            "projects/my-proj/secrets/db-password/versions/latest"
        );
    }

    #[test]
    fn parse_short_form_rejected_without_default_project() {
        let err = parse_gcp_sm_reference("gcp-sm:bare", None).unwrap_err();
        assert!(format!("{err:?}").contains("GOOGLE_CLOUD_PROJECT"));
    }

    #[test]
    fn parse_rejects_missing_scheme() {
        let err = parse_gcp_sm_reference("env:foo", None).unwrap_err();
        assert!(format!("{err:?}").contains("gcp-sm scheme expected"));
    }

    #[test]
    fn parse_rejects_empty_body() {
        let err = parse_gcp_sm_reference("gcp-sm:", None).unwrap_err();
        assert!(format!("{err:?}").contains("missing the secret reference"));
    }

    #[test]
    fn parse_rejects_malformed_long_form() {
        // Wrong segment count.
        let err = parse_gcp_sm_reference(
            "gcp-sm:projects/p/secrets/s",
            None,
        )
        .unwrap_err();
        assert!(format!("{err:?}").contains("invalid resource path"));
        // Wrong literal ("buckets" not "secrets").
        let err = parse_gcp_sm_reference(
            "gcp-sm:projects/p/buckets/s/versions/1",
            None,
        )
        .unwrap_err();
        assert!(format!("{err:?}").contains("invalid resource path"));
        // Empty segment.
        let err = parse_gcp_sm_reference(
            "gcp-sm:projects/p/secrets//versions/1",
            None,
        )
        .unwrap_err();
        assert!(format!("{err:?}").contains("invalid resource path"));
    }

    #[test]
    fn parse_short_form_rejects_invalid_secret_id() {
        // Short form goes through the secret-id whitelist (a `/` in
        // a short id would be ambiguous with the long form).
        let err = parse_gcp_sm_reference("gcp-sm:bad/id", Some("p")).unwrap_err();
        assert!(format!("{err:?}").contains("invalid resource path"));
        // Other unsupported chars.
        let err = parse_gcp_sm_reference("gcp-sm:has space", Some("p")).unwrap_err();
        assert!(format!("{err:?}").contains("invalid secret id"));
    }

    #[test]
    fn cache_ttl_empty_string_disables() {
        assert!(parse_cache_ttl("").unwrap().is_none());
    }

    #[test]
    fn cache_ttl_zero_disables() {
        assert!(parse_cache_ttl("0").unwrap().is_none());
    }

    #[test]
    fn cache_ttl_parses_seconds() {
        assert_eq!(
            parse_cache_ttl("300").unwrap(),
            Some(Duration::from_secs(300))
        );
    }

    #[test]
    fn cache_ttl_rejects_garbage() {
        let err = parse_cache_ttl("five").unwrap_err();
        assert!(format!("{err:?}").contains("not a valid u64"));
    }

    #[test]
    fn status_error_403_distinguishes_permission_denied() {
        let err = secret_manager_status_error(
            "projects/p/secrets/s/versions/1",
            403,
            "{\"error\":\"...\"}",
        );
        assert!(format!("{err:?}").contains("permission denied"));
    }

    #[test]
    fn status_error_404_distinguishes_not_found() {
        let err = secret_manager_status_error(
            "projects/p/secrets/missing/versions/1",
            404,
            "{}",
        );
        let msg = format!("{err:?}");
        assert!(msg.contains("not found"));
    }
}
