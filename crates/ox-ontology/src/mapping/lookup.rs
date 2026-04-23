//! Read-side lookup trait — `node_id → relation` and
//! `(node_id, property_id) → column` questions.
//!
//! Phase 4-A migration bridge. The legacy `SourceMapping` flat
//! HashMaps and the canonical `ObjectMappingDef` list answer the
//! same two questions; every current reader only calls those two
//! methods. This trait lets a reader accept either shape without
//! forking the function body.
//!
//! Callers pass `&source_mapping` (legacy) or `&object_mappings[..]`
//! (canonical slice). Inside the function, the two calls dispatch
//! through the trait. When 1.2b wiring lands and the write side
//! emits canonical by default, the legacy argument at each call
//! site can be swapped out without touching the function body.

use crate::mapping::{ObjectMappingDef, PropertyLocation, SourceMapping};

/// Read-side lookup surface shared by the legacy flat mapping and
/// the canonical mapping list.
///
/// The trait is intentionally minimal — only the two lookups every
/// existing reader actually performs. Anything richer (filter,
/// transform, temporal window) belongs on the canonical
/// `ObjectMappingDef` directly; a caller that needs those
/// features should not be going through this trait.
pub trait ObjectMappingLookup {
    /// Relation (source table) a node type maps to, if any.
    fn table_for_node(&self, node_id: &str) -> Option<&str>;

    /// Source column for a `(node, property)` pair, if the property
    /// is backed by a column. Returns `None` when the property maps
    /// to a JSON path or a derived function — the legacy flat shape
    /// collapses both into "no entry", so this implementation
    /// matches that behaviour for cross-path consistency.
    fn column_for_property(&self, node_id: &str, property_id: &str) -> Option<&str>;
}

impl ObjectMappingLookup for SourceMapping {
    fn table_for_node(&self, node_id: &str) -> Option<&str> {
        SourceMapping::table_for_node(self, node_id)
    }

    fn column_for_property(&self, node_id: &str, property_id: &str) -> Option<&str> {
        SourceMapping::column_for_property(self, node_id, property_id)
    }
}

impl ObjectMappingLookup for [ObjectMappingDef] {
    fn table_for_node(&self, node_id: &str) -> Option<&str> {
        self.iter()
            .find(|om| om.node_type_id.as_ref() == node_id)
            .map(|om| om.relation.as_str())
    }

    fn column_for_property(&self, node_id: &str, property_id: &str) -> Option<&str> {
        let om = self
            .iter()
            .find(|om| om.node_type_id.as_ref() == node_id)?;
        let pm = om
            .property_mappings
            .iter()
            .find(|pm| pm.property_id.as_ref() == property_id)?;
        match &pm.location {
            PropertyLocation::Column(col) => Some(col.column.as_str()),
            PropertyLocation::JsonPath { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mapping::SourceId;
    use ox_core::property_key::PropertyKey;

    fn key(s: &str) -> PropertyKey {
        PropertyKey::new(s).expect("valid")
    }

    fn legacy_fixture() -> SourceMapping {
        let mut m = SourceMapping::new();
        m.node_tables
            .insert("customer".to_string(), "customers".to_string());
        m.node_tables
            .insert("order".to_string(), "orders".to_string());
        m.set_column("customer", "email", "email_addr".to_string());
        m.set_column("customer", "name", "full_name".to_string());
        m.set_column("order", "total", "total_amount".to_string());
        m
    }

    fn canonical_fixture() -> Vec<ObjectMappingDef> {
        legacy_fixture()
            .to_canonical(&SourceId::new("pg-main"), |_, prop_id| Some(key(prop_id)))
            .expect("conversion")
    }

    #[test]
    fn source_mapping_trait_impl_matches_inherent_methods() {
        let m = legacy_fixture();
        // The trait delegates to the inherent methods, so both
        // paths return identical values. The property is just a
        // guardrail — if a refactor accidentally decouples the two,
        // this test fails.
        assert_eq!(
            <SourceMapping as ObjectMappingLookup>::table_for_node(&m, "customer"),
            m.table_for_node("customer"),
        );
        assert_eq!(
            <SourceMapping as ObjectMappingLookup>::column_for_property(&m, "customer", "email"),
            m.column_for_property("customer", "email"),
        );
    }

    #[test]
    fn canonical_slice_answers_table_lookups() {
        let oms = canonical_fixture();
        assert_eq!(oms.as_slice().table_for_node("customer"), Some("customers"));
        assert_eq!(oms.as_slice().table_for_node("order"), Some("orders"));
        assert_eq!(oms.as_slice().table_for_node("missing"), None);
    }

    #[test]
    fn canonical_slice_answers_column_lookups() {
        let oms = canonical_fixture();
        assert_eq!(
            oms.as_slice().column_for_property("customer", "email"),
            Some("email_addr"),
        );
        assert_eq!(
            oms.as_slice().column_for_property("customer", "name"),
            Some("full_name"),
        );
        assert_eq!(
            oms.as_slice().column_for_property("customer", "missing"),
            None,
        );
        assert_eq!(
            oms.as_slice().column_for_property("missing", "email"),
            None,
        );
    }

    #[test]
    fn canonical_slice_treats_json_path_locations_as_no_column() {
        use crate::mapping::{PropertyMappingDef, PropertyTransform};
        use crate::ir::{NodeTypeId, PropertyId};
        // Hand-build an ObjectMappingDef whose sole PropertyMapping
        // points at a JSON path — the legacy flat shape has no way
        // to represent that, so the canonical slice must answer
        // `None` for the column query to stay behaviour-equivalent.
        let om = ObjectMappingDef {
            id: crate::mapping::ObjectMappingId::new("om-1"),
            node_type_id: NodeTypeId::new("node-a"),
            source_id: SourceId::new("pg-main"),
            relation: "docs".into(),
            relation_kind: crate::mapping::SourceRelationKind::default(),
            primary_key_columns: Vec::new(),
            row_filter: None,
            property_mappings: vec![PropertyMappingDef {
                property_id: PropertyId::new("prop-zip"),
                property_key: key("zip"),
                location: PropertyLocation::JsonPath {
                    root_column: "address".into(),
                    path: "postal_code".into(),
                },
                transform: PropertyTransform::Identity,
            }],
            workspace_scope: None,
            precedence: u8::MAX,
            valid_from: None,
            valid_to: None,
            cache_hint: crate::mapping::CacheHintKind::default(),
        };
        let oms = vec![om];
        assert_eq!(oms.as_slice().table_for_node("node-a"), Some("docs"));
        assert_eq!(oms.as_slice().column_for_property("node-a", "prop-zip"), None);
    }

    #[test]
    fn legacy_and_canonical_agree_for_every_known_key() {
        // Cross-check property — enumerate every (node, prop) pair
        // from the legacy blob and confirm both implementations
        // return the same answer, including for the orphan / not-
        // in-set case handled by the closing `None` assertion.
        let legacy = legacy_fixture();
        let canonical = canonical_fixture();
        let pairs: &[(&str, &str)] = &[
            ("customer", "email"),
            ("customer", "name"),
            ("order", "total"),
            ("customer", "missing"),
            ("missing", "email"),
        ];
        for (node_id, prop_id) in pairs {
            let l = legacy.column_for_property(node_id, prop_id);
            let c = canonical.as_slice().column_for_property(node_id, prop_id);
            assert_eq!(
                l, c,
                "disagreement for ({node_id}, {prop_id}): legacy={l:?}, canonical={c:?}",
            );
        }
    }

    fn accepts_lookup<'a, M: ?Sized + ObjectMappingLookup>(
        m: &'a M,
        node_id: &'a str,
    ) -> Option<&'a str> {
        m.table_for_node(node_id)
    }

    #[test]
    fn generic_bound_accepts_both_shapes() {
        // Type-level assertion: a single helper signature with a
        // `?Sized + ObjectMappingLookup` bound must accept both
        // `&SourceMapping` and `&[ObjectMappingDef]`. If a future
        // refactor adds a `Sized` requirement this test stops
        // compiling, exposing the regression immediately.
        let sm = legacy_fixture();
        let oms = canonical_fixture();
        assert_eq!(accepts_lookup(&sm, "customer"), Some("customers"));
        assert_eq!(
            accepts_lookup(oms.as_slice(), "customer"),
            Some("customers"),
        );
    }
}
