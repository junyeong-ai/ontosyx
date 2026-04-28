//! Application Default Credentials dispatch.
//!
//! GCP exposes three credential sources that any client library has to
//! traverse in a fixed order:
//!
//! 1. An explicit caller-provided file path (`override_path`). Lets a
//!    project pin a specific service-account key without polluting the
//!    process environment.
//! 2. `GOOGLE_APPLICATION_CREDENTIALS` env var pointing at a JSON
//!    credential file.
//! 3. The gcloud-managed default file at
//!    `$HOME/.config/gcloud/application_default_credentials.json`,
//!    populated by `gcloud auth application-default login`.
//! 4. The GCE / GKE workload-identity metadata server.
//!
//! For 1–3, the JSON file's `type` field determines whether it is a
//! service-account key or an authorized-user secret — the two formats
//! require different `yup-oauth2` authenticator builders. `yup-oauth2`'s
//! own `ApplicationDefaultCredentialsAuthenticator` only handles the
//! service-account / metadata-server pair, so the authorized-user case
//! has to be dispatched explicitly.
//!
//! [`detect_adc`] performs the dispatch *without* doing any token
//! endpoint I/O. It returns the kind of credential found, leaving
//! authenticator construction to the caller — most useful for
//! `gcp-bigquery-client`, which has its own `Client::from_*`
//! constructors per credential kind. [`build_authenticator`] is the
//! companion that takes the same enum and produces a ready-to-use
//! `yup-oauth2` `Authenticator`, suitable for callers (Secret Manager,
//! future REST clients) that mint bearer tokens themselves.

use std::path::{Path, PathBuf};

use hyper_util::client::legacy::connect::HttpConnector;
use thiserror::Error;
use yup_oauth2::authenticator::{ApplicationDefaultCredentialsTypes, Authenticator};
use yup_oauth2::hyper_rustls::HttpsConnector;
use yup_oauth2::{
    ApplicationDefaultCredentialsAuthenticator, ApplicationDefaultCredentialsFlowOpts,
    AuthorizedUserAuthenticator, ServiceAccountAuthenticator,
};

/// The kind of credential that ADC dispatch resolved to.
#[derive(Debug, Clone)]
pub enum AdcCredential {
    /// `{"type": "service_account", ...}` — long-lived key file.
    ServiceAccountKey(PathBuf),
    /// `{"type": "authorized_user", ...}` — written by
    /// `gcloud auth application-default login`.
    AuthorizedUser(PathBuf),
    /// No file. Token is minted via the GCE / GKE metadata server.
    Metadata,
}

impl AdcCredential {
    /// File path on disk, or `None` for the metadata-server variant.
    pub fn path(&self) -> Option<&Path> {
        match self {
            Self::ServiceAccountKey(p) | Self::AuthorizedUser(p) => Some(p),
            Self::Metadata => None,
        }
    }
}

/// Ready-to-use `yup-oauth2` authenticator. Token requests still need
/// scope strings supplied per call (`auth.token(&[scope]).await`).
pub type GcpAuthenticator = Authenticator<HttpsConnector<HttpConnector>>;

#[derive(Debug, Error)]
pub enum GcpAuthError {
    #[error("credential file `{path}` does not exist")]
    NotFound { path: PathBuf },
    #[error("credential file `{path}` could not be read: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("credential file `{path}` is not valid JSON: {source}")]
    InvalidJson {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error(
        "credential file `{path}` has unsupported `type` field {found:?}; \
         expected `service_account` or `authorized_user`"
    )]
    UnsupportedType {
        path: PathBuf,
        found: Option<String>,
    },
    #[error("authenticator construction failed: {0}")]
    Build(String),
}

/// Resolve which ADC credential to use, without contacting any token
/// endpoint. Returns the credential kind so the caller can pick the
/// matching client-library constructor.
///
/// Dispatch order: `override_path` → `GOOGLE_APPLICATION_CREDENTIALS`
/// env → gcloud default file → metadata-server fallback.
pub async fn detect_adc(override_path: Option<&Path>) -> Result<AdcCredential, GcpAuthError> {
    if let Some(path) = override_path {
        return classify_file(path).await;
    }

    if let Some(env_path) = std::env::var("GOOGLE_APPLICATION_CREDENTIALS")
        .ok()
        .filter(|s| !s.is_empty())
    {
        return classify_file(Path::new(&env_path)).await;
    }

    if let Some(adc_path) = gcloud_adc_path()
        && tokio::fs::try_exists(&adc_path).await.unwrap_or(false)
    {
        return classify_file(&adc_path).await;
    }

    Ok(AdcCredential::Metadata)
}

/// Construct a `yup-oauth2` authenticator for the given credential.
/// Most callers use this; BQ-specific consumers prefer to dispatch on
/// [`AdcCredential`] directly because `gcp-bigquery-client` provides
/// per-kind constructors.
pub async fn build_authenticator(adc: AdcCredential) -> Result<GcpAuthenticator, GcpAuthError> {
    match adc {
        AdcCredential::ServiceAccountKey(path) => {
            let key = yup_oauth2::read_service_account_key(&path)
                .await
                .map_err(|source| GcpAuthError::Io {
                    path: path.clone(),
                    source,
                })?;
            ServiceAccountAuthenticator::builder(key)
                .build()
                .await
                .map_err(|e| {
                    GcpAuthError::Build(format!(
                        "service-account authenticator from `{}`: {e}",
                        path.display()
                    ))
                })
        }
        AdcCredential::AuthorizedUser(path) => {
            let secret = yup_oauth2::read_authorized_user_secret(&path)
                .await
                .map_err(|source| GcpAuthError::Io {
                    path: path.clone(),
                    source,
                })?;
            AuthorizedUserAuthenticator::builder(secret)
                .build()
                .await
                .map_err(|e| {
                    GcpAuthError::Build(format!(
                        "authorized-user authenticator from `{}`: {e}",
                        path.display()
                    ))
                })
        }
        AdcCredential::Metadata => {
            let opts = ApplicationDefaultCredentialsFlowOpts::default();
            match ApplicationDefaultCredentialsAuthenticator::builder(opts).await {
                ApplicationDefaultCredentialsTypes::InstanceMetadata(builder) => builder
                    .build()
                    .await
                    .map_err(|e| GcpAuthError::Build(format!("metadata authenticator: {e}"))),
                ApplicationDefaultCredentialsTypes::ServiceAccount(builder) => {
                    builder.build().await.map_err(|e| {
                        GcpAuthError::Build(format!("ADC service-account authenticator: {e}"))
                    })
                }
            }
        }
    }
}

async fn classify_file(path: &Path) -> Result<AdcCredential, GcpAuthError> {
    if !tokio::fs::try_exists(path).await.unwrap_or(false) {
        return Err(GcpAuthError::NotFound {
            path: path.to_path_buf(),
        });
    }
    let content = tokio::fs::read_to_string(path)
        .await
        .map_err(|source| GcpAuthError::Io {
            path: path.to_path_buf(),
            source,
        })?;
    let json: serde_json::Value =
        serde_json::from_str(&content).map_err(|source| GcpAuthError::InvalidJson {
            path: path.to_path_buf(),
            source,
        })?;
    let kind = json
        .get("type")
        .and_then(|v| v.as_str())
        .map(str::to_owned);
    match kind.as_deref() {
        Some("service_account") => Ok(AdcCredential::ServiceAccountKey(path.to_path_buf())),
        Some("authorized_user") => Ok(AdcCredential::AuthorizedUser(path.to_path_buf())),
        _ => Err(GcpAuthError::UnsupportedType {
            path: path.to_path_buf(),
            found: kind,
        }),
    }
}

fn gcloud_adc_path() -> Option<PathBuf> {
    let home = std::env::var("HOME").ok().filter(|s| !s.is_empty())?;
    Some(PathBuf::from(home).join(".config/gcloud/application_default_credentials.json"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    async fn write_credential(dir: &TempDir, name: &str, contents: &str) -> PathBuf {
        let path = dir.path().join(name);
        tokio::fs::write(&path, contents).await.unwrap();
        path
    }

    #[tokio::test]
    async fn override_authorized_user_is_classified() {
        let dir = TempDir::new().unwrap();
        let path = write_credential(
            &dir,
            "adc.json",
            r#"{
                "type": "authorized_user",
                "client_id": "x",
                "client_secret": "y",
                "refresh_token": "z"
            }"#,
        )
        .await;
        let adc = detect_adc(Some(&path)).await.unwrap();
        assert!(matches!(adc, AdcCredential::AuthorizedUser(p) if p == path));
    }

    #[tokio::test]
    async fn override_service_account_is_classified() {
        let dir = TempDir::new().unwrap();
        let path = write_credential(
            &dir,
            "sa.json",
            r#"{
                "type": "service_account",
                "project_id": "demo",
                "private_key_id": "k",
                "private_key": "-----BEGIN PRIVATE KEY-----\n-----END PRIVATE KEY-----",
                "client_email": "demo@demo.iam.gserviceaccount.com",
                "client_id": "1",
                "token_uri": "https://oauth2.googleapis.com/token"
            }"#,
        )
        .await;
        let adc = detect_adc(Some(&path)).await.unwrap();
        assert!(matches!(adc, AdcCredential::ServiceAccountKey(p) if p == path));
    }

    #[tokio::test]
    async fn override_unsupported_type_rejected() {
        let dir = TempDir::new().unwrap();
        let path = write_credential(
            &dir,
            "bad.json",
            r#"{"type": "external_account"}"#,
        )
        .await;
        let err = detect_adc(Some(&path)).await.unwrap_err();
        assert!(matches!(
            err,
            GcpAuthError::UnsupportedType { found: Some(ref t), .. } if t == "external_account"
        ));
    }

    #[tokio::test]
    async fn override_missing_path_rejected() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("does-not-exist.json");
        let err = detect_adc(Some(&path)).await.unwrap_err();
        assert!(matches!(err, GcpAuthError::NotFound { .. }));
    }

    #[tokio::test]
    async fn override_invalid_json_rejected() {
        let dir = TempDir::new().unwrap();
        let path = write_credential(&dir, "broken.json", "not json").await;
        let err = detect_adc(Some(&path)).await.unwrap_err();
        assert!(matches!(err, GcpAuthError::InvalidJson { .. }));
    }
}
