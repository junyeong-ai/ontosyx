//! Neo4j graph runtime backend.
//!
//! Module layout:
//! - [`runtime`] — `Neo4jRuntime` struct, connection setup, GraphRuntime impl
//! - [`load`]    — bulk load execution (UNWIND batches + retry)
//! - [`search`]  — graph exploration (search_nodes, expand_node, graph_overview)
//! - [`transience`] — Neo4j-specific transient error detection
//! - [`retry`]   — exponential backoff helper
//! - [`helpers`] — parameter binding and value conversion utilities

mod helpers;
mod load;
mod retry;
mod runtime;
mod search;
mod transience;

pub use runtime::Neo4jRuntime;
pub use transience::Neo4jTransienceDetector;

#[cfg(test)]
mod tests {
    use crate::LoadBatch;
    use ox_core::error::OxError;
    use serde_json::json;

    // -----------------------------------------------------------------------
    // LoadBatch tests (defined in lib.rs, tested here for convenience)
    // -----------------------------------------------------------------------

    #[test]
    fn test_load_batch_valid_objects() {
        let values = vec![
            json!({"name": "Alice", "age": 30}),
            json!({"name": "Bob", "age": 25}),
        ];
        let batch = LoadBatch::from_values(values).expect("valid objects should be accepted");
        assert_eq!(batch.len(), 2);
        assert!(!batch.is_empty());

        let records = batch.records();
        assert_eq!(records[0].get("name").unwrap(), "Alice");
        assert_eq!(records[1].get("name").unwrap(), "Bob");
    }

    #[test]
    fn test_load_batch_rejects_non_objects() {
        for (value, kind) in [
            (json!([1, 2, 3]), "array"),
            (json!("just a string"), "string"),
            (json!(null), "null"),
            (json!(42), "number"),
            (json!(true), "boolean"),
        ] {
            let err = LoadBatch::from_values(vec![value]).unwrap_err();
            match err {
                OxError::Validation { field, message } => {
                    assert_eq!(field, "batch[0]");
                    assert!(
                        message.contains(kind),
                        "message should mention '{kind}': {message}"
                    );
                }
                other => panic!("Expected Validation error, got {other:?}"),
            }
        }

        // Mixed: valid object then invalid
        let values = vec![json!({"valid": true}), json!("invalid")];
        let err = LoadBatch::from_values(values).unwrap_err();
        match err {
            OxError::Validation { field, .. } => {
                assert_eq!(
                    field, "batch[1]",
                    "should report index of the failing element"
                );
            }
            other => panic!("Expected Validation error, got {other:?}"),
        }
    }

    #[test]
    fn test_load_batch_empty_is_ok() {
        let batch = LoadBatch::from_values(vec![]).expect("empty vec should be valid");
        assert!(batch.is_empty());
        assert_eq!(batch.len(), 0);
        assert!(batch.records().is_empty());
    }

    #[test]
    fn test_load_batch_into_records() {
        let values = vec![json!({"x": 1}), json!({"y": 2})];
        let batch = LoadBatch::from_values(values).unwrap();
        let records = batch.into_records();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].get("x").unwrap(), 1);
        assert_eq!(records[1].get("y").unwrap(), 2);
    }
}
