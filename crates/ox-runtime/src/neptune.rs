// ---------------------------------------------------------------------------
// NeptuneRuntime — stub for Amazon Neptune openCypher backend
// ---------------------------------------------------------------------------
// Neptune supports a subset of openCypher via HTTPS + IAM Sig4 auth.
//
// Key differences from Neo4j:
// - No Bolt protocol — HTTPS endpoint only
// - No shortestPath() — path algorithms require Gremlin
// - No MERGE ON CREATE/ON MATCH — limited MERGE support
// - No CREATE INDEX — indexes are automatic
// - No APOC/GDS plugins — use Neptune Analytics instead
//
// This is a **stub** implementation. Operations return clear error messages
// indicating that full Neptune support requires AWS SDK + IAM configuration.
// The stub allows the registry to accept "neptune" as a backend type and
// validates endpoint configuration at construction time.
// ---------------------------------------------------------------------------

use std::collections::HashMap;

use async_trait::async_trait;

use ox_core::error::{OxError, OxResult};
use ox_core::query_ir::QueryResult;
use ox_core::types::PropertyValue;

use crate::{GraphRuntime, LoadBatch, LoadResult, SandboxHandle, TransienceDetector};

// ---------------------------------------------------------------------------
// NeptuneTransienceDetector
// ---------------------------------------------------------------------------

/// Neptune-specific transient error detection.
/// Covers HTTPS transport errors and Neptune throttling responses.
pub struct NeptuneTransienceDetector;

impl TransienceDetector for NeptuneTransienceDetector {
    fn is_transient(&self, err_msg: &str) -> bool {
        let lower = err_msg.to_lowercase();
        lower.contains("throttling")
            || lower.contains("too many requests")
            || lower.contains("service unavailable")
            || lower.contains("connection reset")
            || lower.contains("timed out")
            || lower.contains("timeout")
            || lower.contains("internal server error")
    }
}

// ---------------------------------------------------------------------------
// NeptuneRuntime
// ---------------------------------------------------------------------------

/// Stub runtime for Amazon Neptune's openCypher endpoint.
///
/// Validates endpoint and region at construction time. All execution methods
/// return descriptive errors until the AWS SDK integration is implemented.
#[derive(Debug)]
pub struct NeptuneRuntime {
    endpoint: String,
    region: String,
}

impl NeptuneRuntime {
    /// Create a new Neptune runtime stub.
    ///
    /// `endpoint` should be the Neptune cluster's openCypher HTTPS endpoint,
    /// e.g. `https://<cluster-id>.<region>.neptune.amazonaws.com:8182/openCypher`.
    ///
    /// `region` is the AWS region, e.g. `us-east-1`.
    pub fn new(endpoint: &str, region: &str) -> OxResult<Self> {
        if endpoint.is_empty() {
            return Err(OxError::Validation {
                field: "endpoint".to_string(),
                message: "Neptune endpoint URL is required".to_string(),
            });
        }
        if region.is_empty() {
            return Err(OxError::Validation {
                field: "region".to_string(),
                message: "AWS region is required for Neptune".to_string(),
            });
        }
        Ok(Self {
            endpoint: endpoint.to_string(),
            region: region.to_string(),
        })
    }

    /// Return the configured Neptune endpoint.
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    /// Return the configured AWS region.
    pub fn region(&self) -> &str {
        &self.region
    }

    fn not_implemented(&self, operation: &str) -> OxError {
        OxError::Runtime {
            message: format!(
                "Neptune openCypher: '{operation}' is not yet implemented. \
                 Configure AWS credentials and enable the neptune feature. \
                 Endpoint: {}, Region: {}",
                self.endpoint, self.region,
            ),
        }
    }
}

#[async_trait]
impl GraphRuntime for NeptuneRuntime {
    async fn execute_schema(&self, _statements: &[String]) -> OxResult<()> {
        // Neptune manages indexes automatically — schema DDL is a no-op.
        // Constraints (uniqueness, existence) are not supported in Neptune
        // openCypher, so we skip them silently.
        tracing::debug!(
            "Neptune: schema DDL skipped (indexes are automatic, constraints unsupported)"
        );
        Ok(())
    }

    async fn execute_query(
        &self,
        _query: &str,
        _params: &HashMap<String, PropertyValue>,
    ) -> OxResult<QueryResult> {
        Err(self.not_implemented("execute_query"))
    }

    async fn execute_load(&self, _query: &str, _batch: LoadBatch) -> OxResult<LoadResult> {
        Err(self.not_implemented("execute_load"))
    }

    async fn create_sandbox(&self, _name: &str) -> OxResult<SandboxHandle> {
        // Neptune doesn't support multiple databases or dynamic namespacing.
        Err(OxError::UnsupportedOperation {
            target: "neptune".to_string(),
            operation: "create_sandbox (Neptune has no database-level isolation)".to_string(),
        })
    }

    async fn drop_sandbox(&self, _handle: &SandboxHandle) -> OxResult<()> {
        Err(OxError::UnsupportedOperation {
            target: "neptune".to_string(),
            operation: "drop_sandbox (Neptune has no database-level isolation)".to_string(),
        })
    }

    fn runtime_name(&self) -> &str {
        "neptune"
    }

    async fn health_check(&self) -> bool {
        // Full health check requires HTTPS + IAM auth.
        // Stub always returns false.
        false
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_validates_endpoint() {
        let err = NeptuneRuntime::new("", "us-east-1").unwrap_err();
        assert!(err.to_string().contains("endpoint"));
    }

    #[test]
    fn new_validates_region() {
        let err = NeptuneRuntime::new("https://example.com", "").unwrap_err();
        assert!(err.to_string().contains("region"));
    }

    #[test]
    fn new_succeeds_with_valid_params() {
        let rt = NeptuneRuntime::new(
            "https://my-cluster.us-east-1.neptune.amazonaws.com:8182/openCypher",
            "us-east-1",
        )
        .unwrap();
        assert_eq!(
            rt.endpoint(),
            "https://my-cluster.us-east-1.neptune.amazonaws.com:8182/openCypher"
        );
        assert_eq!(rt.region(), "us-east-1");
    }

    #[test]
    fn runtime_name_is_neptune() {
        let rt = NeptuneRuntime::new("https://example.com", "us-east-1").unwrap();
        assert_eq!(rt.runtime_name(), "neptune");
    }

    #[tokio::test]
    async fn health_check_returns_false() {
        let rt = NeptuneRuntime::new("https://example.com", "us-east-1").unwrap();
        assert!(!rt.health_check().await);
    }

    #[tokio::test]
    async fn execute_schema_is_noop() {
        let rt = NeptuneRuntime::new("https://example.com", "us-east-1").unwrap();
        // Schema DDL should succeed silently (Neptune auto-indexes)
        rt.execute_schema(&["CREATE INDEX ...".to_string()])
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn execute_query_returns_not_implemented() {
        let rt = NeptuneRuntime::new("https://example.com", "us-east-1").unwrap();
        let err = rt
            .execute_query("MATCH (n) RETURN n", &HashMap::new())
            .await
            .unwrap_err();
        assert!(err.to_string().contains("not yet implemented"));
    }

    #[tokio::test]
    async fn create_sandbox_unsupported() {
        let rt = NeptuneRuntime::new("https://example.com", "us-east-1").unwrap();
        let err = rt.create_sandbox("test").await.unwrap_err();
        assert!(matches!(err, OxError::UnsupportedOperation { .. }));
    }

    #[test]
    fn transience_detector_classifies_correctly() {
        let detector = NeptuneTransienceDetector;
        assert!(detector.is_transient("ThrottlingException: Rate exceeded"));
        assert!(detector.is_transient("connection reset by peer"));
        assert!(detector.is_transient("Request timed out"));
        assert!(!detector.is_transient("SyntaxError: Invalid query"));
        assert!(!detector.is_transient("ConstraintViolation: duplicate"));
    }
}
