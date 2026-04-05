use async_trait::async_trait;

use ox_core::error::{OxError, OxResult};
use ox_core::source_schema::{SourceProfile, SourceSchema};

use crate::{AnalysisResult, DataSourceIntrospector};

/// Snowflake data source introspector (stub).
///
/// Snowflake does not yet have a mature, production-ready Rust client crate.
/// The `snowflake-connector` crate (0.4.0) exists but lacks the stability and
/// feature coverage needed for reliable schema introspection.
///
/// This stub validates connection parameters and returns a clear error directing
/// users to the Snowflake REST SQL API integration path. It is registered in the
/// `IntrospectorRegistry` so that the frontend can display Snowflake as a source
/// option and collect credentials ahead of the full implementation.
///
/// ## Connection parameters
///
/// - `account`   — Snowflake account identifier (e.g., `xy12345.us-east-1`)
/// - `user`      — Login username
/// - `password`  — Login password
/// - `warehouse` — Compute warehouse name
/// - `database`  — Target database
/// - `schema`    — Target schema within the database
///
/// ## Future implementation path
///
/// Snowflake exposes a REST SQL API at
/// `POST https://{account}.snowflakecomputing.com/api/v2/statements`
/// which can be called via `reqwest` with key-pair or password-based JWT auth.
/// The introspection queries are standard `INFORMATION_SCHEMA` queries identical
/// to PostgreSQL, making the migration from stub to real implementation
/// straightforward.
#[derive(Debug)]
pub struct SnowflakeIntrospector {
    _account: String,
    _user: String,
    _warehouse: String,
    _database: String,
    _schema: String,
}

impl SnowflakeIntrospector {
    /// Parse Snowflake connection parameters from the registry `SourceInput` fields.
    ///
    /// Expected `connection_string` format:
    /// `snowflake://{account}/{database}/{schema}?warehouse={warehouse}`
    ///
    /// `schema_name` is used as the user credential pair: `user:password`.
    ///
    /// Alternatively, individual fields can be packed into the connection string
    /// as a URL and extracted here.
    pub fn from_params(
        account: &str,
        user: &str,
        _password: &str,
        warehouse: &str,
        database: &str,
        schema: &str,
    ) -> OxResult<Self> {
        if account.is_empty() {
            return Err(OxError::Validation {
                field: "account".to_string(),
                message: "Snowflake account identifier is required".to_string(),
            });
        }
        if user.is_empty() {
            return Err(OxError::Validation {
                field: "user".to_string(),
                message: "Snowflake user is required".to_string(),
            });
        }
        if database.is_empty() {
            return Err(OxError::Validation {
                field: "database".to_string(),
                message: "Snowflake database is required".to_string(),
            });
        }

        Ok(Self {
            _account: account.to_string(),
            _user: user.to_string(),
            _warehouse: warehouse.to_string(),
            _database: database.to_string(),
            _schema: schema.to_string(),
        })
    }

    /// Parse a Snowflake connection string in the format:
    /// `snowflake://{account}/{database}/{schema}?user={user}&password={password}&warehouse={warehouse}`
    ///
    /// Uses manual parsing to avoid adding a `url` crate dependency to ox-source.
    pub fn from_connection_string(connection_string: &str) -> OxResult<Self> {
        let cs = connection_string.trim();
        let expected_format = "snowflake://{account}/{database}/{schema}\
                               ?user={user}&password={password}&warehouse={warehouse}";

        let rest = cs
            .strip_prefix("snowflake://")
            .ok_or_else(|| OxError::Validation {
                field: "connection_string".to_string(),
                message: format!(
                    "Expected 'snowflake://' scheme. Expected format: {expected_format}"
                ),
            })?;

        // Split path?query
        let (path_part, query_part) = match rest.split_once('?') {
            Some((p, q)) => (p, q),
            None => (rest, ""),
        };

        // Path: {account}/{database}/{schema}
        let segments: Vec<&str> = path_part.split('/').collect();
        let account = segments.first().unwrap_or(&"").to_string();
        let database = segments.get(1).unwrap_or(&"").to_string();
        let schema = if segments.len() > 2 && !segments[2].is_empty() {
            segments[2].to_string()
        } else {
            "PUBLIC".to_string()
        };

        // Query params: key=value&key=value
        let params: std::collections::HashMap<&str, &str> = query_part
            .split('&')
            .filter(|s| !s.is_empty())
            .filter_map(|pair| pair.split_once('='))
            .collect();

        let user = params.get("user").unwrap_or(&"").to_string();
        let password = params.get("password").unwrap_or(&"").to_string();
        let warehouse = params.get("warehouse").unwrap_or(&"").to_string();

        Self::from_params(&account, &user, &password, &warehouse, &database, &schema)
    }

    fn stub_error() -> OxError {
        OxError::Runtime {
            message: "Snowflake connector is not yet fully implemented. \
                      No mature Rust-native Snowflake client crate is available. \
                      Use the Snowflake REST SQL API integration \
                      (POST https://{account}.snowflakecomputing.com/api/v2/statements) \
                      with reqwest for production use."
                .to_string(),
        }
    }
}

#[async_trait]
impl DataSourceIntrospector for SnowflakeIntrospector {
    fn source_type(&self) -> &str {
        "snowflake"
    }

    async fn introspect_schema(&self) -> OxResult<SourceSchema> {
        Err(Self::stub_error())
    }

    async fn collect_stats(&self, _schema: &SourceSchema) -> OxResult<SourceProfile> {
        Err(Self::stub_error())
    }

    async fn analyze(&self) -> OxResult<AnalysisResult> {
        Err(Self::stub_error())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_valid_connection_string() {
        let cs = "snowflake://xy12345.us-east-1/MY_DB/MY_SCHEMA?user=alice&password=secret&warehouse=COMPUTE_WH";
        let introspector = SnowflakeIntrospector::from_connection_string(cs).unwrap();
        assert_eq!(introspector._account, "xy12345.us-east-1");
        assert_eq!(introspector._database, "MY_DB");
        assert_eq!(introspector._schema, "MY_SCHEMA");
        assert_eq!(introspector._warehouse, "COMPUTE_WH");
        assert_eq!(introspector._user, "alice");
    }

    #[test]
    fn parse_connection_string_defaults_schema_to_public() {
        let cs = "snowflake://xy12345/MY_DB?user=alice&password=secret&warehouse=WH";
        let introspector = SnowflakeIntrospector::from_connection_string(cs).unwrap();
        assert_eq!(introspector._schema, "PUBLIC");
    }

    #[test]
    fn parse_connection_string_wrong_scheme() {
        let cs = "postgres://host/db";
        let result = SnowflakeIntrospector::from_connection_string(cs);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("snowflake://"),
            "Error should mention expected scheme: {err}"
        );
    }

    #[test]
    fn from_params_validates_required_fields() {
        let result = SnowflakeIntrospector::from_params("", "user", "pass", "wh", "db", "schema");
        assert!(result.is_err());

        let result = SnowflakeIntrospector::from_params("acct", "", "pass", "wh", "db", "schema");
        assert!(result.is_err());

        let result = SnowflakeIntrospector::from_params("acct", "user", "pass", "wh", "", "schema");
        assert!(result.is_err());
    }

    #[test]
    fn source_type_returns_snowflake() {
        let introspector =
            SnowflakeIntrospector::from_params("acct", "user", "pass", "wh", "db", "schema")
                .unwrap();
        assert_eq!(introspector.source_type(), "snowflake");
    }

    #[tokio::test]
    async fn introspect_returns_stub_error() {
        let introspector =
            SnowflakeIntrospector::from_params("acct", "user", "pass", "wh", "db", "schema")
                .unwrap();
        let result = introspector.introspect_schema().await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("not yet fully implemented"), "Error: {err}");
    }
}
