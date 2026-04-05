use async_trait::async_trait;
use tracing::info;

use ox_core::error::{OxError, OxResult};
use ox_core::source_schema::{SourceProfile, SourceSchema};

use crate::DataSourceIntrospector;

/// BigQuery introspector stub.
///
/// BigQuery uses a project/dataset/table hierarchy.  A full implementation
/// would connect via the BigQuery REST API (e.g., `gcp-bigquery-client` crate)
/// and discover tables + column schemas within the given dataset.
///
/// This stub parses the connection URI, validates the required fields, and
/// returns a clear error at introspection time so the registry entry and
/// frontend form work end-to-end even before the SDK dependency is wired in.
pub struct BigQueryIntrospector {
    project_id: String,
    dataset: String,
    #[allow(dead_code)]
    credentials_path: Option<String>,
}

impl BigQueryIntrospector {
    /// Parse a BigQuery connection URI and return a (stub) introspector.
    ///
    /// Expected format: `bigquery://PROJECT_ID/DATASET[?credentials_path=PATH]`
    ///
    /// Authentication:
    /// - If `credentials_path` query parameter is set, use that service account JSON file.
    /// - Otherwise, fall back to the `GOOGLE_APPLICATION_CREDENTIALS` environment variable.
    pub async fn connect(connection_string: &str) -> OxResult<Self> {
        let (project_id, dataset, credentials_path) = parse_bigquery_uri(connection_string)?;

        info!(
            project_id = %project_id,
            dataset = %dataset,
            credentials = credentials_path.as_deref().unwrap_or("(env default)"),
            "Parsed BigQuery connection config"
        );

        Ok(Self {
            project_id,
            dataset,
            credentials_path,
        })
    }
}

#[async_trait]
impl DataSourceIntrospector for BigQueryIntrospector {
    fn source_type(&self) -> &str {
        "bigquery"
    }

    async fn introspect_schema(&self) -> OxResult<SourceSchema> {
        Err(OxError::Runtime {
            message: format!(
                "BigQuery introspection is not yet implemented. \
                 Target: project={}, dataset={}. \
                 A full implementation requires the `gcp-bigquery-client` crate \
                 or direct REST API integration.",
                self.project_id, self.dataset
            ),
        })
    }

    async fn collect_stats(&self, _schema: &SourceSchema) -> OxResult<SourceProfile> {
        Err(OxError::Runtime {
            message: "BigQuery data profiling is not yet implemented.".to_string(),
        })
    }
}

// ---------------------------------------------------------------------------
// URI parsing
// ---------------------------------------------------------------------------

/// Parse `bigquery://PROJECT_ID/DATASET[?credentials_path=PATH]` into components.
fn parse_bigquery_uri(uri: &str) -> OxResult<(String, String, Option<String>)> {
    let trimmed = uri.trim();

    if !trimmed.starts_with("bigquery://") {
        return Err(OxError::Validation {
            field: "connection_string".to_string(),
            message: format!(
                "BigQuery connection string must start with 'bigquery://'. Got: {trimmed}"
            ),
        });
    }

    // Use url crate for robust parsing (handles percent-encoding, query params, etc.)
    let url = url::Url::parse(trimmed).map_err(|e| OxError::Validation {
        field: "connection_string".to_string(),
        message: format!("Invalid BigQuery URI: {e}"),
    })?;

    let project_id = url.host_str().unwrap_or("").to_string();
    if project_id.is_empty() {
        return Err(OxError::Validation {
            field: "connection_string".to_string(),
            message: "BigQuery URI missing project_id (expected bigquery://PROJECT_ID/DATASET)"
                .to_string(),
        });
    }

    let dataset = url.path().trim_start_matches('/').to_string();
    if dataset.is_empty() {
        return Err(OxError::Validation {
            field: "connection_string".to_string(),
            message: "BigQuery URI missing dataset (expected bigquery://PROJECT_ID/DATASET)"
                .to_string(),
        });
    }

    let credentials_path = url
        .query_pairs()
        .find(|(k, _)| k == "credentials_path")
        .map(|(_, v)| v.to_string());

    Ok((project_id, dataset, credentials_path))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_basic_uri() {
        let (project, dataset, creds) =
            parse_bigquery_uri("bigquery://my-gcp-project/analytics_prod").unwrap();
        assert_eq!(project, "my-gcp-project");
        assert_eq!(dataset, "analytics_prod");
        assert!(creds.is_none());
    }

    #[test]
    fn parse_uri_with_credentials() {
        let (project, dataset, creds) =
            parse_bigquery_uri("bigquery://my-project/my_dataset?credentials_path=/etc/sa.json")
                .unwrap();
        assert_eq!(project, "my-project");
        assert_eq!(dataset, "my_dataset");
        assert_eq!(creds.as_deref(), Some("/etc/sa.json"));
    }

    #[test]
    fn parse_missing_scheme_is_error() {
        let result = parse_bigquery_uri("my-project/my_dataset");
        assert!(result.is_err());
    }

    #[test]
    fn parse_missing_dataset_is_error() {
        let result = parse_bigquery_uri("bigquery://my-project/");
        assert!(result.is_err());
    }

    #[test]
    fn parse_missing_project_is_error() {
        let result = parse_bigquery_uri("bigquery:///my_dataset");
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn introspect_returns_stub_error() {
        let introspector = BigQueryIntrospector::connect("bigquery://test-project/test_dataset")
            .await
            .unwrap();

        assert_eq!(introspector.source_type(), "bigquery");

        let err = introspector.introspect_schema().await.unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("not yet implemented"),
            "unexpected error: {msg}"
        );
        assert!(msg.contains("test-project"));
        assert!(msg.contains("test_dataset"));
    }
}
