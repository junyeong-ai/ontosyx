use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Maps ontology entities back to their source data origins.
/// Kept separate from OntologyIR to maintain clean graph schema definitions.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SourceMapping {
    /// node_type_id → source table name
    pub node_tables: HashMap<String, String>,
    /// "node_type_id/property_id" → source column name
    pub property_columns: HashMap<String, String>,
}

impl SourceMapping {
    pub fn new() -> Self {
        Self::default()
    }

    /// Build the composite key for property_columns.
    fn property_key(node_id: &str, property_id: &str) -> String {
        format!("{node_id}/{property_id}")
    }

    /// Insert a source column mapping for a (node, property) pair.
    pub fn set_column(&mut self, node_id: &str, property_id: &str, column: String) {
        self.property_columns
            .insert(Self::property_key(node_id, property_id), column);
    }

    /// Get the source table for a node type
    pub fn table_for_node(&self, node_id: &str) -> Option<&str> {
        self.node_tables.get(node_id).map(|s| s.as_str())
    }

    /// Get the source column for a property
    pub fn column_for_property(&self, node_id: &str, property_id: &str) -> Option<&str> {
        self.property_columns
            .get(&Self::property_key(node_id, property_id))
            .map(|s| s.as_str())
    }

    /// Whether this mapping has any node table entries.
    pub fn has_node_tables(&self) -> bool {
        !self.node_tables.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_source_mapping_json_roundtrip() {
        let mut mapping = SourceMapping::new();
        mapping
            .node_tables
            .insert("node-product".to_string(), "products".to_string());
        mapping
            .node_tables
            .insert("node-customer".to_string(), "customers".to_string());
        mapping.set_column("node-product", "prop-name", "product_name".to_string());
        mapping.set_column("node-customer", "prop-email", "email_address".to_string());

        // Full JSON roundtrip must succeed (was broken with tuple keys)
        let json_val = serde_json::to_value(&mapping).unwrap();
        let roundtripped: SourceMapping = serde_json::from_value(json_val).unwrap();
        assert_eq!(roundtripped.node_tables, mapping.node_tables);
        assert_eq!(roundtripped.property_columns, mapping.property_columns);

        // Verify accessors after roundtrip
        assert_eq!(
            roundtripped.column_for_property("node-product", "prop-name"),
            Some("product_name")
        );
        assert_eq!(
            roundtripped.column_for_property("node-customer", "prop-email"),
            Some("email_address")
        );
    }

    #[test]
    fn test_source_mapping_accessors() {
        let mut mapping = SourceMapping::new();
        mapping
            .node_tables
            .insert("node-product".to_string(), "products".to_string());
        mapping.set_column("node-product", "prop-sku", "sku_code".to_string());

        // table_for_node
        assert_eq!(mapping.table_for_node("node-product"), Some("products"));
        assert_eq!(mapping.table_for_node("node-nonexistent"), None);

        // column_for_property
        assert_eq!(
            mapping.column_for_property("node-product", "prop-sku"),
            Some("sku_code")
        );
        assert_eq!(
            mapping.column_for_property("node-product", "prop-missing"),
            None
        );
        assert_eq!(
            mapping.column_for_property("node-missing", "prop-sku"),
            None
        );

        // has_node_tables
        assert!(mapping.has_node_tables());
        let empty = SourceMapping::new();
        assert!(!empty.has_node_tables());
    }
}
