//! `InterfaceDef` — abstract contract for a set of node types.
//!
//! Interfaces solve the "same capability, different concrete types"
//! problem without forcing an inheritance hierarchy.  A query that
//! targets `(:HasAddress)` expands across every `NodeTypeDef` whose
//! `implements` list contains the `HasAddress` id.  Concrete node
//! types keep their own labels, properties, and edges — the
//! interface only names the required property / edge slots every
//! implementer must provide.
//!
//! Design mirrors Palantir Foundry's "Object Type Interfaces" and the
//! LinkML `mixin` concept: a fair-game marker that lets the planner
//! reason over a union of types without forcing a named superclass.
//! An interface is *not* a node type — you cannot create an instance
//! of `HasAddress` directly; you create instances of concrete types
//! that implement it.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use ox_core::graph_label::GraphLabel;
use ox_core::i18n::LocalizedText;
use ox_core::property_key::PropertyKey;
use ox_core::types::PropertyType;

use crate::ir::{EdgeTypeId, PropertyId};

ox_core::define_id_newtype!(
    /// Stable identifier for an `InterfaceDef`. A node type declares
    /// the interfaces it fulfils by listing these ids in its
    /// `implements` collection.
    InterfaceId
);

/// Declarative interface over node types.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct InterfaceDef {
    pub id: InterfaceId,

    /// Canonical, Cypher-safe label. Displayed in the query surface
    /// when a user pins a query to the interface rather than a
    /// concrete node type.
    pub label: GraphLabel,

    #[serde(default)]
    pub display_name: LocalizedText,

    #[serde(default)]
    pub description: LocalizedText,

    /// Properties every implementer must expose. The node type
    /// validator rejects implementers that lack any of these or
    /// whose matching property has an incompatible type.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_properties: Vec<InterfaceProperty>,

    /// Edge types an implementer must connect through. Similar
    /// validation semantics — an implementer missing a declared
    /// edge is rejected.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_edges: Vec<InterfaceEdge>,
}

/// A property that every implementer of the interface must expose.
///
/// The interface names the *shape* (name + required type + required
/// nullability); the implementer's concrete property id can differ
/// — the validator matches by `name`, not by `PropertyId`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct InterfaceProperty {
    /// Property key as it appears on the implementer's node.
    pub name: PropertyKey,
    pub property_type: PropertyType,
    /// Whether implementers are allowed to mark this property
    /// nullable. `false` means the implementer's property must be
    /// non-nullable; `true` means either is acceptable.
    #[serde(default)]
    pub nullable: bool,
    /// Optional id the interface author may pin when they want to
    /// reference the same property id across implementers. Usually
    /// left unset — matching is by name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_property_id: Option<PropertyId>,
}

/// An edge type an implementer must connect through.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct InterfaceEdge {
    /// Edge label the implementer must participate in (typically as
    /// source). The planner matches by label because edge ids are
    /// not meaningful across implementers.
    pub label: GraphLabel,
    /// Whether the implementer must be the *source* side of the
    /// edge. When `false`, both source and target count as
    /// satisfying the interface.
    #[serde(default)]
    pub as_source: bool,
    /// Optional pinned id — same opt-in as `InterfaceProperty`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_edge_type_id: Option<EdgeTypeId>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gl(s: &'static str) -> GraphLabel {
        GraphLabel::new(s).expect("valid")
    }

    #[test]
    fn minimal_interface_serialises_without_optionals() {
        let iface = InterfaceDef {
            id: InterfaceId::new("if-addressable"),
            label: gl("HasAddress"),
            display_name: LocalizedText::default(),
            description: LocalizedText::default(),
            required_properties: vec![],
            required_edges: vec![],
        };
        let j = serde_json::to_string(&iface).unwrap();
        // No `required_properties` / `required_edges` when empty —
        // keeps the on-wire shape compact for common cases.
        assert!(!j.contains("required_properties"));
        assert!(!j.contains("required_edges"));
    }

    #[test]
    fn interface_roundtrips_properties_and_edges() {
        let iface = InterfaceDef {
            id: InterfaceId::new("if-priced"),
            label: gl("Priced"),
            display_name: LocalizedText::default(),
            description: LocalizedText::default(),
            required_properties: vec![InterfaceProperty {
                name: PropertyKey::new("price").expect("valid"),
                property_type: PropertyType::Float,
                nullable: false,
                expected_property_id: None,
            }],
            required_edges: vec![InterfaceEdge {
                label: gl("PRICED_IN"),
                as_source: true,
                expected_edge_type_id: None,
            }],
        };
        let j = serde_json::to_value(&iface).unwrap();
        let back: InterfaceDef = serde_json::from_value(j).unwrap();
        assert_eq!(back, iface);
    }
}
