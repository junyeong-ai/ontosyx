//! `PropertyMappingDef` — the leaf of the mapping tree.
//!
//! A property mapping names one property on an ontology node type or
//! edge type and explains how to materialise that property from the
//! physical source bound by the enclosing `ObjectMappingDef`.
//!
//! Two axes:
//! 1. **Where** the value lives (column, JSON path, nested doc key).
//! 2. **How** the value is transformed on the way out (identity,
//!    rename only, SQL expression, JSON-path traversal, or a derived
//!    function reference).
//!
//! The two axes are separate by design. A `ColumnRef` with a SQL
//! transform (`UPPER(col)`) is a common case; a `JsonPath` with a
//! derived function (mapping a nested document through a computed
//! field) is unusual but representable. Keeping the concerns
//! orthogonal avoids a combinatorial enum explosion.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use ox_core::property_key::PropertyKey;

use crate::ir::PropertyId;
use crate::mapping::refs::ColumnRef;

/// Binding from an ontology property to a physical value location.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
pub struct PropertyMappingDef {
    /// Property this binding targets. Must reference a `PropertyId`
    /// owned by the `NodeTypeDef` or `EdgeTypeDef` that the enclosing
    /// mapping is bound to — validated at mapping registration.
    pub property_id: PropertyId,

    /// For display + debugging. The property key used by the
    /// ontology (not the source column). Kept alongside `property_id`
    /// so a mapping export is readable without re-resolving against
    /// the ontology.
    pub property_key: PropertyKey,

    /// Where the value comes from.
    pub location: PropertyLocation,

    /// How the value is shaped on extraction. `Identity` by default —
    /// explicit so that a round-trip through the wire format is
    /// lossless, and so the planner can detect pushdown-safe bindings
    /// by pattern-matching on `Identity`.
    #[serde(default)]
    pub transform: PropertyTransform,

    /// Optional [`ConceptMapDef`](crate::concept_map::ConceptMapDef)
    /// applied at query-compile time so the upstream source's codes
    /// land on the property's canonical vocabulary without authors
    /// repeating the mapping at every callsite. The compiler's
    /// `concept_map_rewrite` walker reads this id when assembling
    /// its `(variable, property) → ConceptMapDef` translation
    /// table.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub concept_map_id: Option<crate::concept_map::ConceptMapId>,
}

/// Physical-side location of a property value.
///
/// A sum type rather than two separate `Option` fields so that every
/// mapping has exactly one storage origin — a future `ExternalCall`
/// variant (REST / gRPC) can slot in without breaking the wire
/// format of existing bindings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PropertyLocation {
    /// A column in the mapping's relation.
    Column(ColumnRef),
    /// A JSON path into a document or JSON-typed column. The first
    /// segment names the column (or document field) the path starts
    /// in; subsequent segments drill into nested structure.
    JsonPath {
        /// Column or field the path is anchored on.
        root_column: String,
        /// Dotted JSON path ending at the value — e.g. `"address.zip"`.
        path: String,
    },
}

/// Value transform applied on the way from the source to the
/// ontology. Identity is the common case; richer transforms let a
/// mapping handle legacy schemas without dragging ETL into the
/// platform.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PropertyTransform {
    /// The source value is the property value — no coercion, no
    /// rename. Pushdown-safe for every adapter.
    #[default]
    Identity,
    /// Source expression evaluated by the source's own engine.
    /// Example: `"UPPER(name)"`, `"quantity * unit_price"`. The
    /// planner passes this through to the source dialect verbatim,
    /// so the author's expression must be valid for that dialect.
    SqlExpr { expression: String },
    /// Derived property resolved by an `ox_ontology::FunctionDef`.
    /// Kept as a string id because `FunctionDef` is not yet part of
    /// the ontology model; the id will become a typed `FunctionId`
    /// once the function registry lands.
    Derived { function_id: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn column_location_roundtrips() {
        let m = PropertyMappingDef {
            property_id: PropertyId::new("prop-email"),
            property_key: PropertyKey::new("email").expect("valid"),
            location: PropertyLocation::Column(ColumnRef::new("customers", "email_addr")),
            transform: PropertyTransform::Identity,
            concept_map_id: None,
        };
        let j = serde_json::to_value(&m).unwrap();
        let back: PropertyMappingDef = serde_json::from_value(j).unwrap();
        assert_eq!(back, m);
    }

    #[test]
    fn json_path_location_carries_anchor_and_path() {
        let m = PropertyMappingDef {
            property_id: PropertyId::new("prop-zip"),
            property_key: PropertyKey::new("zip").expect("valid"),
            location: PropertyLocation::JsonPath {
                root_column: "address".into(),
                path: "postal_code".into(),
            },
            transform: PropertyTransform::Identity,
            concept_map_id: None,
        };
        let j = serde_json::to_value(&m).unwrap();
        let back: PropertyMappingDef = serde_json::from_value(j).unwrap();
        assert_eq!(back, m);
    }

    #[test]
    fn sql_expr_transform_round_trips_verbatim() {
        let m = PropertyMappingDef {
            property_id: PropertyId::new("prop-total"),
            property_key: PropertyKey::new("total").expect("valid"),
            location: PropertyLocation::Column(ColumnRef::new("orders", "raw")),
            transform: PropertyTransform::SqlExpr {
                expression: "qty * unit_price".into(),
            },
            concept_map_id: None,
        };
        let j = serde_json::to_string(&m).unwrap();
        assert!(j.contains("\"qty * unit_price\""));
    }

    #[test]
    fn identity_is_the_default_transform() {
        let t = PropertyTransform::default();
        assert!(matches!(t, PropertyTransform::Identity));
    }
}
