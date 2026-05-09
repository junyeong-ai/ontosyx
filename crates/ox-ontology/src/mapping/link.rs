//! `LinkMappingDef` — binding from one `EdgeTypeDef` to the
//! relation(s) that supply edges of that type.
//!
//! Three shapes cover every real-world edge topology without losing
//! planner-relevant distinctions:
//!
//! - **ForeignKey** — the edge is a direct FK on the source node's
//!   relation. One relation access yields every edge.
//! - **Bridge** — many-to-many; a separate bridge relation holds
//!   `(source_pk, target_pk)` pairs.
//! - **Computed** — the edge is produced by a SQL expression over
//!   one relation (self-join, predicate-based matching).
//! - **Federated** — the source endpoint and the target endpoint
//!   live in *different* sources. The planner emits a bloom-filter
//!   hash join and flags the cost.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::ir::EdgeTypeId;
use crate::mapping::refs::{ColumnRef, LinkMappingId, SourceId, SourceRelationRef};

/// Binding from an `EdgeTypeDef` to a physical relation (or across
/// several relations).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
pub struct LinkMappingDef {
    pub id: LinkMappingId,
    pub edge_type_id: EdgeTypeId,

    /// Shape of the binding — see module-level docs for when to pick
    /// which variant.
    pub kind: LinkMappingKind,

    /// Where the source (tail) endpoint lives.
    pub source_endpoint: EndpointRef,

    /// Where the target (head) endpoint lives.
    pub target_endpoint: EndpointRef,

    /// Hint the planner consults when ordering joins. The scalar is
    /// an adapter-reported estimate, not a precise cardinality.
    #[serde(default)]
    pub join_cost_hint: JoinCostHint,

    /// Higher precedence wins in multi-mapping dedup on the same
    /// edge type. Mirrors `ObjectMappingDef::precedence`.
    #[serde(default)]
    pub precedence: u8,

    /// Π-2: Semantic cardinality of the edge. **Correctness-critical
    /// for NL2SQL**: a `ManyToMany` traversal without an explicit
    /// `DISTINCT` at the aggregation step inflates row counts,
    /// making `SUM`/`COUNT` silently 2-3× wrong. The compiler
    /// consults this field and injects `DISTINCT` automatically
    /// when a query aggregates across a many-side link.
    ///
    /// Distinct from [`JoinCostHint`] which is a **performance**
    /// signal: cardinality is **semantic correctness**. The two
    /// axes are orthogonal — a `ManyToMany` edge may be cheap
    /// (tiny bridge table) and a `ManyToOne` edge may be expensive
    /// (large source table).
    ///
    /// Defaults per kind follow the conservative choice — picking
    /// `ManyToMany` over-estimates (adds a redundant `DISTINCT`)
    /// but never under-estimates (which would produce the
    /// inflation bug). Authors override when they know better.
    ///
    /// Reference: dbt Semantic Layer `many_to_many`, Cube.js
    /// relationships `hasMany` / `belongsTo`, OWL
    /// `FunctionalProperty` / `InverseFunctionalProperty`.
    #[serde(default)]
    pub cardinality: LinkCardinality,
}

/// Π-2: Semantic cardinality of a link (edge) — drives compiler
/// decisions about when to inject `DISTINCT` in generated SQL.
///
/// The four values form the standard relational-algebra cross
/// product (one/many on each side). Naming matches the dbt /
/// Cube.js / LookML conventions so imports from those systems
/// round-trip unchanged.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum LinkCardinality {
    /// Exactly one target per source, exactly one source per
    /// target. Example: `User.profile` where both sides are 1:1.
    OneToOne,
    /// One source maps to many targets. Example: `Customer` →
    /// `Order` (one customer has many orders).
    OneToMany,
    /// Many sources map to one target. Example: `Order` →
    /// `Customer` (many orders share one customer). This is the
    /// typical FK direction.
    ManyToOne,
    /// Many-to-many. Example: `Product` ↔ `Category` via a bridge
    /// table. Aggregation across this edge **requires** `DISTINCT`
    /// to avoid row-duplication.
    #[default]
    ManyToMany,
}

impl LinkCardinality {
    /// Conservative default for a [`LinkMappingKind`] when the
    /// author does not specify. `ManyToOne` for direct FK edges
    /// (the common FK direction — N orders → 1 customer);
    /// `ManyToMany` for bridges / computed / federated (no
    /// uniqueness guarantee on either side without extra proof).
    pub fn default_for(kind: &LinkMappingKind) -> Self {
        match kind {
            LinkMappingKind::ForeignKey { .. } => LinkCardinality::ManyToOne,
            LinkMappingKind::Bridge { .. }
            | LinkMappingKind::Computed { .. }
            | LinkMappingKind::Federated { .. } => LinkCardinality::ManyToMany,
        }
    }

    /// `true` when a query aggregating over this edge must inject
    /// `DISTINCT` at the aggregation step — row fan-out from a
    /// many-side traversal would otherwise double-count.
    pub fn requires_distinct_on_aggregation(&self) -> bool {
        matches!(
            self,
            LinkCardinality::OneToMany | LinkCardinality::ManyToMany
        )
    }
}

impl LinkMappingDef {
    /// Report whether the binding crosses source boundaries. Used by
    /// the planner to pick a bloom-filter hash-join plan instead of
    /// a source-native join.
    pub fn crosses_sources(&self) -> bool {
        matches!(self.kind, LinkMappingKind::Federated { .. })
            || self.source_endpoint.source_id != self.target_endpoint.source_id
    }
}

/// Shape variants for a link mapping.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LinkMappingKind {
    /// Direct FK on the source relation. `join_columns` names the
    /// source-side column and the target-side column the FK targets;
    /// both already live in their respective endpoints' relations.
    ForeignKey {
        source_column: ColumnRef,
        target_column: ColumnRef,
    },
    /// Many-to-many via a bridge relation. `bridge_relation` is the
    /// intermediate table / collection; `source_join` and
    /// `target_join` each hold the sequence of bridge-side columns
    /// the planner zips against the endpoint's `key_columns` to
    /// build one `AND`-combined equi-predicate per side.
    ///
    /// The vectors must be the same length as the opposite
    /// endpoint's `key_columns`. A single-column PK therefore uses a
    /// one-element vec; a composite `(region, warehouse)` PK uses a
    /// two-element vec pairing bridge columns in the matching order.
    ///
    /// `bridge_workspace_scope` makes the bridge relation
    /// workspace-isolated when populated. The federation planner
    /// emits an extra equi-predicate on this column against the
    /// caller's `WorkspaceScope`, so a multi-tenant bridge table
    /// (one row set per workspace) doesn't leak rows across the
    /// scope boundary. `None` declares a workspace-agnostic bridge —
    /// only safe when the bridge holds no workspace-private joins.
    Bridge {
        bridge_relation: SourceRelationRef,
        source_join: Vec<ColumnRef>,
        target_join: Vec<ColumnRef>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        bridge_workspace_scope: Option<ColumnRef>,
    },
    /// Edge produced by a SQL predicate over one or both endpoint
    /// relations. The predicate is evaluated in the source's dialect;
    /// the planner does not translate it across dialects.
    Computed { predicate: String },
    /// The endpoints are in different sources. There is no
    /// source-native join path; the planner materialises both sides
    /// into Arrow and joins engine-side.
    Federated {
        source_match_column: ColumnRef,
        target_match_column: ColumnRef,
    },
}

/// Resolvable reference to an endpoint of a link. Either `ObjectMappingId`
/// (the endpoint is already bound) or a bare `(source, relation,
/// columns)` tuple for endpoints that are mapped inline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
pub struct EndpointRef {
    pub source_id: SourceId,
    pub relation: String,
    /// Column(s) whose values identify the endpoint instance. Must
    /// match the endpoint object mapping's `primary_key_columns`
    /// (validated at registration).
    pub key_columns: Vec<String>,
}

/// Adapter-reported join-cost hint. Coarse on purpose — a richer
/// cost estimator belongs to the planner.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum JoinCostHint {
    #[default]
    Unknown,
    /// Indexed on both sides, expected nested-loop-cheap.
    Indexed,
    /// Moderate join: one side indexed, the other scanned.
    Scan,
    /// Expensive cross join / no index support. The planner warns
    /// and, when federation is in play, may recommend caching.
    Cartesian,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ep(source: &str, relation: &str, col: &str) -> EndpointRef {
        EndpointRef {
            source_id: SourceId::new(source),
            relation: relation.into(),
            key_columns: vec![col.into()],
        }
    }

    #[test]
    fn same_source_foreign_key_does_not_cross_sources() {
        let lm = LinkMappingDef {
            id: LinkMappingId::new("lm-1"),
            edge_type_id: EdgeTypeId::new("e-placed"),
            kind: LinkMappingKind::ForeignKey {
                source_column: ColumnRef::new("orders", "customer_id"),
                target_column: ColumnRef::new("customers", "id"),
            },
            source_endpoint: ep("pg-main", "orders", "customer_id"),
            target_endpoint: ep("pg-main", "customers", "id"),
            join_cost_hint: JoinCostHint::Indexed,
            precedence: 100,
            cardinality: LinkCardinality::ManyToOne,
        };
        assert!(!lm.crosses_sources());
    }

    #[test]
    fn federated_kind_or_mixed_sources_cross() {
        let lm = LinkMappingDef {
            id: LinkMappingId::new("lm-2"),
            edge_type_id: EdgeTypeId::new("e-owns"),
            kind: LinkMappingKind::Federated {
                source_match_column: ColumnRef::new("users", "email"),
                target_match_column: ColumnRef::new("accounts", "owner_email"),
            },
            source_endpoint: ep("pg-main", "users", "email"),
            target_endpoint: ep("snowflake-dw", "accounts", "owner_email"),
            join_cost_hint: JoinCostHint::Scan,
            precedence: 100,
            cardinality: LinkCardinality::ManyToMany,
        };
        assert!(lm.crosses_sources());
    }

    #[test]
    fn roundtrips_through_json() {
        let lm = LinkMappingDef {
            id: LinkMappingId::new("lm-bridge"),
            edge_type_id: EdgeTypeId::new("e-tagged"),
            kind: LinkMappingKind::Bridge {
                bridge_relation: SourceRelationRef {
                    source_id: SourceId::new("pg-main"),
                    relation: "post_tags".into(),
                    kind: Default::default(),
                },
                source_join: vec![ColumnRef::new("post_tags", "post_id")],
                target_join: vec![ColumnRef::new("post_tags", "tag_id")],
                bridge_workspace_scope: None,
            },
            source_endpoint: ep("pg-main", "posts", "id"),
            target_endpoint: ep("pg-main", "tags", "id"),
            join_cost_hint: JoinCostHint::Indexed,
            precedence: 50,
            cardinality: LinkCardinality::ManyToMany,
        };
        let j = serde_json::to_value(&lm).unwrap();
        let back: LinkMappingDef = serde_json::from_value(j).unwrap();
        assert_eq!(back, lm);
    }
}
