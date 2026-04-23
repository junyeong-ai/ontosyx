//! `SegmentDef` — named, reusable rule-filter over a single
//! `NodeType`.
//!
//! A `SegmentDef` lets an operator declare a domain concept like
//! "VIP customers" or "at-risk orders" **once**, as a filter
//! expression over property values, and reuse it everywhere:
//!
//! - in queries via `MATCH (c:Customer) WHERE @segment:vip(c)`,
//! - in rules via `ShaclConstraint::Disjoint` targets,
//! - in dashboards as a ready-made subset of a type.
//!
//! The definition is **domain-agnostic** — the struct names
//! generic primitives (property comparisons, And/Or/Not) and holds
//! no domain vocabulary. "VIP" is just a `SegmentDef { id: "vip",
//! target: Customer, filter: property "total_spend" > 1_000_000 }`
//! instance authored by the operator; the platform itself does
//! not privilege "VIP" over "weekly_reader" or any other segment.
//!
//! ## Conceptual reference
//!
//! - **SQL `CREATE VIEW`** — a named subset of a relation. A
//!   segment is the logical equivalent for a node type.
//! - **dbt `semantic_model`**, **Looker LookML `dimension`** — a
//!   named, reusable slice of a dimension set. `SegmentDef` keeps
//!   the filter authoring in the ontology (not in SQL files) so
//!   the LLM prompt layer and the admin UI see the same
//!   definition the runtime evaluates.
//!
//! ## Shape
//!
//! ```text
//! SegmentDef {
//!   id, name, display_name, description,
//!   target_node_type_id: NodeTypeId,
//!   filter: SegmentFilter,       // tree of AST nodes
//!   overlap_policy: OverlapPolicy,
//!   refresh_policy: SegmentRefreshPolicy,
//! }
//! ```
//!
//! `SegmentFilter` is a small AST specific to segment predicates —
//! deliberately **not** `Expr` from ox-query-ir, because a segment
//! definition lives in the ontology (persisted with the schema,
//! seeded by admins) and must not depend on the query-compile
//! crate. The two node shapes that matter — property comparisons
//! and boolean composition — are all the admin UI needs to
//! express "members of this segment" declaratively.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use ox_core::i18n::LocalizedText;
use ox_core::property_key::PropertyKey;

use crate::ir::NodeTypeId;

ox_core::define_id_newtype!(
    /// Stable identifier for a [`SegmentDef`].
    SegmentId
);

/// A named subset of a single `NodeType`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SegmentDef {
    pub id: SegmentId,
    pub name: String,
    #[serde(default)]
    pub display_name: LocalizedText,
    #[serde(default)]
    pub description: LocalizedText,
    pub target_node_type_id: NodeTypeId,
    pub filter: SegmentFilter,
    #[serde(default)]
    pub overlap_policy: OverlapPolicy,
    #[serde(default)]
    pub refresh_policy: SegmentRefreshPolicy,
}

/// Filter AST for segment membership. Internal-only: segment
/// definitions persist with the ontology and are translated to
/// `Expr`s at query time, but the authoring surface is kept
/// minimal.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SegmentFilter {
    And { children: Vec<SegmentFilter> },
    Or { children: Vec<SegmentFilter> },
    Not { inner: Box<SegmentFilter> },
    Equals {
        property: PropertyKey,
        value: SegmentLiteral,
    },
    NotEquals {
        property: PropertyKey,
        value: SegmentLiteral,
    },
    GreaterThan {
        property: PropertyKey,
        value: SegmentLiteral,
    },
    GreaterOrEqual {
        property: PropertyKey,
        value: SegmentLiteral,
    },
    LessThan {
        property: PropertyKey,
        value: SegmentLiteral,
    },
    LessOrEqual {
        property: PropertyKey,
        value: SegmentLiteral,
    },
    In {
        property: PropertyKey,
        values: Vec<SegmentLiteral>,
    },
    IsNull { property: PropertyKey },
}

/// Literal value a segment filter may compare against. Kept narrow
/// on purpose — a segment definition is a UI-authored artefact
/// more than a general-purpose expression. Numeric / string /
/// boolean covers every segment the launch partners expressed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SegmentLiteral {
    Int { value: i64 },
    Float { value: f64 },
    String { value: String },
    Bool { value: bool },
}

/// Whether an instance may belong to multiple segments at once.
/// `Allow` is the default — segments are orthogonal views, an
/// instance can be both "high-spend" and "recent" without
/// conflict. `Disjoint` lets callers declare that the named
/// segments partition the node type; a downstream validator can
/// then flag an instance that would satisfy two disjoint-partition
/// segments.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OverlapPolicy {
    #[default]
    Allow,
    Disjoint,
}

/// How the runtime refreshes segment membership.
///
/// - `OnDemand` — recompute at query time. Simple, no staleness,
///   most expensive. Default.
/// - `Materialised { ttl_seconds }` — cache the member set; refresh
///   when the TTL expires. The runtime rejects writes to a
///   segment-bound property when the cache is stale by more than
///   `2 * ttl_seconds`, to prevent unbounded staleness from a
///   misconfigured scheduler.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema,
)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SegmentRefreshPolicy {
    #[default]
    OnDemand,
    Materialised {
        ttl_seconds: u64,
    },
}

impl SegmentDef {
    /// Walk every `PropertyKey` referenced by the filter. Used by
    /// `OntologyIR::validate()` to confirm every property named by
    /// a segment actually exists on the target node type.
    pub fn referenced_properties(&self) -> Vec<PropertyKey> {
        let mut out = Vec::new();
        walk_properties(&self.filter, &mut out);
        out
    }
}

fn walk_properties(filter: &SegmentFilter, out: &mut Vec<PropertyKey>) {
    match filter {
        SegmentFilter::And { children } | SegmentFilter::Or { children } => {
            for c in children {
                walk_properties(c, out);
            }
        }
        SegmentFilter::Not { inner } => walk_properties(inner, out),
        SegmentFilter::Equals { property, .. }
        | SegmentFilter::NotEquals { property, .. }
        | SegmentFilter::GreaterThan { property, .. }
        | SegmentFilter::GreaterOrEqual { property, .. }
        | SegmentFilter::LessThan { property, .. }
        | SegmentFilter::LessOrEqual { property, .. }
        | SegmentFilter::In { property, .. }
        | SegmentFilter::IsNull { property } => {
            out.push(property.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pk(s: &str) -> PropertyKey {
        PropertyKey::new(s).expect("valid property key")
    }

    #[test]
    fn segment_round_trips_through_json() {
        let seg = SegmentDef {
            id: SegmentId::new("seg-1"),
            name: "vip".into(),
            display_name: LocalizedText::default(),
            description: LocalizedText::default(),
            target_node_type_id: NodeTypeId::new("nt-customer"),
            filter: SegmentFilter::And {
                children: vec![
                    SegmentFilter::GreaterThan {
                        property: pk("total_spend"),
                        value: SegmentLiteral::Int { value: 1_000_000 },
                    },
                    SegmentFilter::Equals {
                        property: pk("country"),
                        value: SegmentLiteral::String {
                            value: "KR".into(),
                        },
                    },
                ],
            },
            overlap_policy: OverlapPolicy::Allow,
            refresh_policy: SegmentRefreshPolicy::Materialised { ttl_seconds: 300 },
        };
        let j = serde_json::to_value(&seg).unwrap();
        let back: SegmentDef = serde_json::from_value(j).unwrap();
        assert_eq!(back, seg);
    }

    #[test]
    fn referenced_properties_walks_nested_filter() {
        let seg = SegmentDef {
            id: SegmentId::new("seg-1"),
            name: "complex".into(),
            display_name: LocalizedText::default(),
            description: LocalizedText::default(),
            target_node_type_id: NodeTypeId::new("nt-customer"),
            filter: SegmentFilter::Or {
                children: vec![
                    SegmentFilter::In {
                        property: pk("tier"),
                        values: vec![
                            SegmentLiteral::String { value: "GOLD".into() },
                            SegmentLiteral::String { value: "PLATINUM".into() },
                        ],
                    },
                    SegmentFilter::Not {
                        inner: Box::new(SegmentFilter::IsNull {
                            property: pk("vip_since"),
                        }),
                    },
                ],
            },
            overlap_policy: OverlapPolicy::Allow,
            refresh_policy: SegmentRefreshPolicy::OnDemand,
        };
        let refs = seg.referenced_properties();
        let names: Vec<&str> = refs.iter().map(|p| p.as_str()).collect();
        assert_eq!(names, vec!["tier", "vip_since"]);
    }

    #[test]
    fn default_policies_match_module_docs() {
        assert!(matches!(OverlapPolicy::default(), OverlapPolicy::Allow));
        assert!(matches!(SegmentRefreshPolicy::default(), SegmentRefreshPolicy::OnDemand));
    }

    #[test]
    fn overlap_policy_disjoint_roundtrips() {
        let j = serde_json::to_value(OverlapPolicy::Disjoint).unwrap();
        let back: OverlapPolicy = serde_json::from_value(j).unwrap();
        assert!(matches!(back, OverlapPolicy::Disjoint));
    }
}
