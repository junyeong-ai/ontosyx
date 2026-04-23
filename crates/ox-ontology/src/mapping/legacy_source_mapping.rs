use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use ox_core::error::{OxError, OxResult};
use ox_core::property_key::PropertyKey;

use crate::ir::{NodeTypeId, PropertyId};
use crate::mapping::object::ObjectMappingDef;
use crate::mapping::property::{PropertyLocation, PropertyMappingDef, PropertyTransform};
use crate::mapping::refs::{ColumnRef, SourceId};

/// Maps ontology entities back to their source data origins.
///
/// **Deprecated (Phase 4-A).** Superseded by `ObjectMappingDef` +
/// `LinkMappingDef` + `PropertyMappingDef` in the sibling modules of
/// this crate — the new types carry temporal windows, precedence,
/// row filters, and workspace-scope column references that this flat
/// shape cannot express. Remaining call-sites migrate in the next
/// slice; once they are gone this module is deleted.
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

    /// Convert this legacy flat mapping into the canonical shape
    /// (`ObjectMappingDef` with nested `PropertyMappingDef`s).
    ///
    /// Migration safety net for Phase 4-A: enables callers to replay
    /// a legacy blob as canonical without touching storage. Additive —
    /// callers that still use `SourceMapping` directly are unaffected.
    ///
    /// The legacy shape cannot express two pieces of information that
    /// the canonical shape requires, so the caller supplies them:
    ///
    /// - `source_id` — legacy does not track which data source the
    ///   tables live in. Caller names it.
    /// - `property_key_for(node_id, property_id)` — legacy stores
    ///   neither the ontology-side property key (the identifier that
    ///   appears inside Cypher) nor the ontology itself. The caller
    ///   resolves the key from the authoritative `OntologyIR`
    ///   (typical use) or fabricates one for tests.
    ///
    /// One `ObjectMappingDef` is emitted per entry in `node_tables`;
    /// `property_columns` entries whose `"node_id/property_id"` key
    /// has no matching node-table entry are silently dropped, since
    /// the legacy type never enforced that invariant on write.
    ///
    /// The output is sorted by `node_type_id` + `property_id` so
    /// two conversions of equivalent blobs compare equal in audit
    /// diffs and snapshot tests.
    ///
    /// Returns an error when the resolver returns `None` for a
    /// `(node_id, property_id)` pair that has a column binding —
    /// the caller decides whether to fall back or fail the migration.
    pub fn to_canonical<F>(
        &self,
        source_id: &SourceId,
        mut property_key_for: F,
    ) -> OxResult<Vec<ObjectMappingDef>>
    where
        F: FnMut(&str, &str) -> Option<PropertyKey>,
    {
        let mut entries: Vec<(&String, &String)> = self.node_tables.iter().collect();
        entries.sort_by(|a, b| a.0.cmp(b.0));

        let mut out = Vec::with_capacity(entries.len());
        for (node_id, relation_str) in entries {
            let relation = relation_str.clone();
            let id = format!("om-legacy-{node_id}");
            let mut om = ObjectMappingDef::new(
                id,
                NodeTypeId::new(node_id.clone()),
                source_id.clone(),
                relation.clone(),
            );

            let prefix = format!("{node_id}/");
            let mut props: Vec<(&str, &String)> = self
                .property_columns
                .iter()
                .filter_map(|(k, v)| {
                    k.strip_prefix(&prefix).and_then(|rem| {
                        // Reject entries whose remainder still contains '/'
                        // so a node_id that is itself a prefix of another
                        // node_id does not swallow the longer match.
                        if rem.contains('/') {
                            None
                        } else {
                            Some((rem, v))
                        }
                    })
                })
                .collect();
            props.sort_by_key(|(rem, _)| *rem);

            for (property_id, column) in props {
                let key = property_key_for(node_id, property_id).ok_or_else(|| {
                    OxError::Validation {
                        field: "property_key".to_string(),
                        message: format!(
                            "cannot resolve property key for `{node_id}/{property_id}` \
                             while converting legacy SourceMapping — resolver returned None"
                        ),
                    }
                })?;
                om.property_mappings.push(PropertyMappingDef {
                    property_id: PropertyId::new(property_id.to_string()),
                    property_key: key,
                    location: PropertyLocation::Column(ColumnRef::new(
                        relation.clone(),
                        column.clone(),
                    )),
                    transform: PropertyTransform::Identity,
                });
            }
            out.push(om);
        }
        Ok(out)
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

    fn key(s: &str) -> PropertyKey {
        PropertyKey::new(s).expect("valid test key")
    }

    #[test]
    fn to_canonical_empty_mapping_yields_empty_vec() {
        let m = SourceMapping::new();
        let out = m
            .to_canonical(&SourceId::new("pg-main"), |_, _| Some(key("ignored")))
            .expect("empty is always ok");
        assert!(out.is_empty());
    }

    #[test]
    fn to_canonical_emits_one_mapping_per_node_table() {
        let mut m = SourceMapping::new();
        m.node_tables
            .insert("customer".to_string(), "customers".to_string());
        m.node_tables
            .insert("order".to_string(), "orders".to_string());
        // No property mappings at all — table-only legacy blob.
        // Resolver returns None; if anything calls it, the conversion
        // errors and `expect` below panics, exposing the bug.
        let out = m
            .to_canonical(&SourceId::new("pg-main"), |_, _| None)
            .expect("no property resolver calls when no columns");
        assert_eq!(out.len(), 2);
        // Sorted by node_type_id for reproducibility.
        assert_eq!(out[0].node_type_id.as_str(), "customer");
        assert_eq!(out[0].relation, "customers");
        assert_eq!(out[0].id.as_str(), "om-legacy-customer");
        assert_eq!(out[0].source_id.as_str(), "pg-main");
        assert!(out[0].property_mappings.is_empty());
        assert_eq!(out[1].node_type_id.as_str(), "order");
        assert_eq!(out[1].relation, "orders");
    }

    #[test]
    fn to_canonical_attaches_property_mappings_to_matching_owner() {
        let mut m = SourceMapping::new();
        m.node_tables
            .insert("customer".to_string(), "customers".to_string());
        m.set_column("customer", "email", "email_addr".to_string());
        m.set_column("customer", "name", "full_name".to_string());

        let out = m
            .to_canonical(&SourceId::new("pg-main"), |node_id, prop_id| {
                assert_eq!(node_id, "customer");
                Some(key(prop_id))
            })
            .expect("conversion succeeds");
        assert_eq!(out.len(), 1);
        let om = &out[0];
        assert_eq!(om.property_mappings.len(), 2);
        // Property mappings are sorted by property_id.
        assert_eq!(om.property_mappings[0].property_id.as_str(), "email");
        assert_eq!(
            om.property_mappings[0].property_key.as_str(),
            "email"
        );
        assert!(matches!(
            &om.property_mappings[0].location,
            PropertyLocation::Column(col)
                if col.relation == "customers" && col.column == "email_addr"
        ));
        assert!(matches!(
            om.property_mappings[0].transform,
            PropertyTransform::Identity
        ));
        assert_eq!(om.property_mappings[1].property_id.as_str(), "name");
    }

    #[test]
    fn to_canonical_drops_orphan_property_columns() {
        let mut m = SourceMapping::new();
        m.node_tables
            .insert("customer".to_string(), "customers".to_string());
        // orphan: no node_tables entry for `order`.
        m.set_column("order", "total", "total_amount".to_string());

        // Same guard as the table-only test: resolver-called would
        // error out and `expect` would panic.
        let out = m
            .to_canonical(&SourceId::new("pg-main"), |_, _| None)
            .expect("orphan columns are silently dropped");
        assert_eq!(out.len(), 1);
        assert!(out[0].property_mappings.is_empty());
    }

    #[test]
    fn to_canonical_fails_when_resolver_returns_none() {
        let mut m = SourceMapping::new();
        m.node_tables
            .insert("customer".to_string(), "customers".to_string());
        m.set_column("customer", "email", "email_addr".to_string());

        let err = m
            .to_canonical(&SourceId::new("pg-main"), |_, _| None)
            .expect_err("unresolved property key must fail");
        assert!(matches!(err, OxError::Validation { ref field, .. } if field == "property_key"));
    }

    #[test]
    fn to_canonical_does_not_confuse_node_ids_that_share_a_prefix() {
        let mut m = SourceMapping::new();
        // Both "a" and "a/b" are registered as distinct node types.
        // A property `a/b/prop1` belongs to `a/b`, not `a`'s "b/prop1".
        m.node_tables.insert("a".to_string(), "tab_a".to_string());
        m.node_tables
            .insert("a/b".to_string(), "tab_ab".to_string());
        m.set_column("a/b", "prop1", "col1".to_string());
        m.set_column("a", "flag", "flag_col".to_string());

        let out = m
            .to_canonical(&SourceId::new("pg-main"), |_, prop_id| {
                Some(key(prop_id))
            })
            .expect("conversion succeeds");
        assert_eq!(out.len(), 2);
        // Sort order: "a" < "a/b".
        assert_eq!(out[0].node_type_id.as_str(), "a");
        assert_eq!(out[0].property_mappings.len(), 1);
        assert_eq!(out[0].property_mappings[0].property_id.as_str(), "flag");
        assert_eq!(out[1].node_type_id.as_str(), "a/b");
        assert_eq!(out[1].property_mappings.len(), 1);
        assert_eq!(out[1].property_mappings[0].property_id.as_str(), "prop1");
    }

    #[test]
    fn to_canonical_roundtrip_matches_legacy_lookup() {
        // Sanity: every legacy column lookup must find a matching
        // canonical PropertyMappingDef with the same column name.
        let mut m = SourceMapping::new();
        m.node_tables
            .insert("customer".to_string(), "customers".to_string());
        m.node_tables
            .insert("order".to_string(), "orders".to_string());
        m.set_column("customer", "email", "email_addr".to_string());
        m.set_column("customer", "sku", "sku_code".to_string());
        m.set_column("order", "total", "total_amount".to_string());

        let out = m
            .to_canonical(&SourceId::new("pg-main"), |_, prop_id| {
                Some(key(prop_id))
            })
            .expect("conversion succeeds");

        for om in &out {
            for pm in &om.property_mappings {
                let expected_col = m
                    .column_for_property(om.node_type_id.as_str(), pm.property_id.as_str())
                    .expect("legacy lookup finds it");
                match &pm.location {
                    PropertyLocation::Column(col) => {
                        assert_eq!(col.column, expected_col);
                        assert_eq!(col.relation, om.relation);
                    }
                    other => panic!("unexpected location shape: {other:?}"),
                }
            }
        }
    }
}
