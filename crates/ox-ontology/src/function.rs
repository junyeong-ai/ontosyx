//! `FunctionDef` — derivations / computed properties / UDFs.
//!
//! A `FunctionDef` names a pure (or marked-impure) computation over
//! ontology values. Two roles today, unified under one type so the
//! wire format stays stable as more roles appear:
//!
//! 1. **Derived property** — `PropertyDef.derived_from: Option<FunctionId>`.
//!    The planner materialises the property by evaluating the
//!    function, rather than reading from a physical column.
//! 2. **User-defined function** — a callable registered on the
//!    federation engine so a raw SQL query (or Cypher) can invoke
//!    `ox_score(…)` against workspace-scoped data.
//!
//! Every function declares:
//!
//! - its `expression` (the body, in a dialect-free enum),
//! - explicit property / edge dependencies so the planner can
//!   schedule evaluation and invalidate downstream caches,
//! - a return type (expressed as a `PropertyType` so the validator
//!   refuses a mapping that binds a string column to an Int result),
//! - a purity tag (`Pure` / `Impure`) so the planner can reason
//!   about memoization.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use ox_core::i18n::LocalizedText;
use ox_core::types::PropertyType;

use crate::ir::{EdgeTypeId, NodeTypeId, PropertyId};

ox_core::define_id_newtype!(
    /// Stable identifier for a `FunctionDef`.
    FunctionId
);

/// Named derivation / UDF.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FunctionDef {
    pub id: FunctionId,

    /// Short, human-readable name. Not required to be a valid
    /// identifier — the compilation step that lowers a function to
    /// a source dialect produces a mangled name from the id.
    pub name: String,

    #[serde(default)]
    pub description: LocalizedText,

    pub expression: FunctionExpression,

    pub return_type: PropertyType,

    /// Whether the planner may memoize calls with equal arguments.
    /// `Impure` functions (today / now / random) never get cached.
    #[serde(default)]
    pub purity: FunctionPurity,

    /// Properties the function reads. Used for cache-invalidation
    /// fan-out — when any listed property changes, memoized results
    /// for this function are dropped.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub property_dependencies: Vec<PropertyDependency>,

    /// Edges the function traverses. Same role as
    /// `property_dependencies` but for relationship presence.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub edge_dependencies: Vec<EdgeTypeId>,
}

/// Reference to a property this function depends on.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PropertyDependency {
    pub node_type_id: NodeTypeId,
    pub property_id: PropertyId,
}

/// Dialect-free function body.
///
/// Each variant carries the shape the compiler needs to emit the
/// appropriate source dialect. `SqlExpr` and `CypherExpr` are
/// verbatim — the author's expression must already be valid for the
/// target. `BuiltIn` names a function the platform implements
/// itself (e.g. `now`, `coalesce`). `Udf` points at a Wasm binary or
/// other externally-built artifact; the content of the binary is
/// stored out-of-band.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FunctionExpression {
    SqlExpr { expression: String },
    CypherExpr { expression: String },
    BuiltIn { name: String },
    Udf { artifact_ref: String },
}

/// Purity tag that drives memoization / plan stability.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FunctionPurity {
    /// Equal arguments → equal output, forever. Safe to memoize.
    #[default]
    Pure,
    /// Output depends on external state (time, randomness, external
    /// call). Planner refuses to memoize.
    Impure,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pure_is_the_default_purity() {
        assert_eq!(FunctionPurity::default(), FunctionPurity::Pure);
    }

    #[test]
    fn sql_expression_round_trips() {
        let f = FunctionDef {
            id: FunctionId::new("fn-total"),
            name: "total".into(),
            description: LocalizedText::default(),
            expression: FunctionExpression::SqlExpr {
                expression: "unit_price * quantity".into(),
            },
            return_type: PropertyType::Float,
            purity: FunctionPurity::Pure,
            property_dependencies: vec![PropertyDependency {
                node_type_id: NodeTypeId::new("nt-order-item"),
                property_id: PropertyId::new("prop-unit-price"),
            }],
            edge_dependencies: vec![],
        };
        let j = serde_json::to_value(&f).unwrap();
        let back: FunctionDef = serde_json::from_value(j).unwrap();
        assert_eq!(back, f);
    }

    #[test]
    fn builtin_expression_keeps_wire_shape_small() {
        let f = FunctionDef {
            id: FunctionId::new("fn-now"),
            name: "now".into(),
            description: LocalizedText::default(),
            expression: FunctionExpression::BuiltIn { name: "now".into() },
            return_type: PropertyType::DateTime,
            purity: FunctionPurity::Impure,
            property_dependencies: vec![],
            edge_dependencies: vec![],
        };
        let j = serde_json::to_string(&f).unwrap();
        // Impure + no deps → compact payload.
        assert!(!j.contains("property_dependencies"));
        assert!(!j.contains("edge_dependencies"));
    }
}
