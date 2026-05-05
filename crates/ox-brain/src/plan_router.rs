//! `PlanRouter` — Cypher / Federation / Hybrid dispatch above the runtime layer.
//!
//! Phase 6 of the long-horizon work plan. The trait + types ship here
//! as a stable contract; the first concrete implementation is a
//! follow-up commit. Until that lands, the agent's `query_graph`
//! tool calls `runtime.execute_query(...)` directly (the existing
//! Cypher-only path); routing through this trait is the seam that
//! enables NL → Federation reach without breaking any existing
//! consumer.
//!
//! See `docs/architecture/plan-router-design.md` for the full
//! decision context, routing heuristics, and integration points.

use async_trait::async_trait;

use ox_core::error::OxResult;
use ox_ontology::ir::OntologyIR;
use ox_query_ir::query::QueryIR;

/// Decide which execution path a `QueryIR` should take.
///
/// Implementations inspect the IR's pattern depth, the
/// ontology's mapping density on the touched node types, and
/// any caller-supplied [`CostBudget`] hints; they MUST be
/// deterministic for the same `(QueryIR, OntologyIR)` pair so
/// plan-cache keys stay stable.
#[async_trait]
pub trait PlanRouter: Send + Sync {
    /// Resolve a routing decision for the supplied IR.
    ///
    /// `budget == None` means "use the workspace-default
    /// cost ceiling"; the implementation reads the workspace
    /// config to pick the actual numbers.
    async fn route(
        &self,
        ir: &QueryIR,
        ontology: &OntologyIR,
        budget: Option<&CostBudget>,
    ) -> OxResult<RouteDecision>;
}

/// Where the platform should execute the IR.
///
/// `Cypher` and `Federation` are leaf decisions; `Hybrid`
/// composes both runtimes in a fixed staged order so a
/// graph-traversal-then-federated-aggregate query stops
/// round-tripping through one runtime that's wrong for half
/// the work.
#[derive(Debug, Clone)]
pub enum RouteDecision {
    /// Single-runtime Cypher execution. The agent calls
    /// `runtime.execute_query(...)` as today; the router's
    /// only contribution is the `reason` (an attribution
    /// string the FE renders alongside the result so the
    /// operator sees *why* the platform picked this path).
    Cypher {
        /// Stable, FE-renderable explanation of the routing
        /// choice (e.g. "graph-native traversal over cached
        /// Neo4j data"). Carried on
        /// `QueryExecution.metadata.routing` so the FE result
        /// panel surfaces it without re-deriving.
        reason: &'static str,
    },

    /// Single-runtime DataFusion execution. The agent calls
    /// `FederationContext::execute_plan(...)` with the
    /// supplied logical plan; per-source data lives in its
    /// own adapter.
    Federation {
        /// Pre-built DataFusion logical plan keyed against the
        /// IR. Stored as `serde_json::Value` rather than a
        /// concrete `datafusion::logical_expr::LogicalPlan`
        /// so this crate stays free of the DataFusion dep —
        /// `ox-federation` is the only crate that links
        /// against it. The federation context deserialises +
        /// optimises before execution.
        plan: serde_json::Value,
        /// FE-renderable explanation, same shape as `Cypher`.
        reason: &'static str,
    },

    /// Multi-runtime composition. Stages execute in order;
    /// each stage's output feeds the next stage's input.
    /// The v1 router emits this only for queries whose IR
    /// genuinely benefits (a graph traversal whose result
    /// set feeds a federated aggregate).
    Hybrid {
        /// Sequence of single-runtime stages, executed in
        /// order. Stage N reads stage N-1's output as a
        /// bound table.
        stages: Vec<HybridStage>,
        reason: &'static str,
    },
}

/// One stage in a `Hybrid` route. Mirrors the leaf variants
/// of [`RouteDecision`] one-for-one.
#[derive(Debug, Clone)]
pub enum HybridStage {
    /// A Cypher sub-query whose result rows feed the next
    /// stage's bound input.
    Cypher { ir: QueryIR },
    /// A DataFusion sub-plan whose result rows feed the
    /// next stage's bound input.
    Federation { plan: serde_json::Value },
}

/// Caller-supplied cost ceiling. Routers refuse the query
/// before execution when the IR's cost estimate exceeds the
/// budget unless the caller opts into high-cost runs.
#[derive(Debug, Clone, Default)]
pub struct CostBudget {
    /// Auto-`LIMIT` injection threshold. `None` means "no
    /// cap on row count" — the router still honours
    /// `allow_high_cost`.
    pub max_rows: Option<u64>,
    /// Wallclock cap. The runtime layer enforces; the
    /// router refuses to dispatch when the IR's estimated
    /// time exceeds this without `allow_high_cost`.
    pub max_wallclock_ms: Option<u64>,
    /// Override for `cost::estimate_cost == RiskLevel::High`.
    /// Default `false`: high-risk queries refuse with a
    /// typed `ApiErrorCode::QueryCostBudgetExceeded` (per
    /// the typed-error contract).
    pub allow_high_cost: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cost_budget_default_refuses_high() {
        let b = CostBudget::default();
        assert!(!b.allow_high_cost);
        assert!(b.max_rows.is_none());
        assert!(b.max_wallclock_ms.is_none());
    }

    #[test]
    fn route_decision_cypher_carries_reason() {
        let d = RouteDecision::Cypher {
            reason: "graph-native traversal over cached Neo4j data",
        };
        match d {
            RouteDecision::Cypher { reason } => {
                assert_eq!(reason, "graph-native traversal over cached Neo4j data")
            }
            _ => panic!("expected Cypher variant"),
        }
    }

    #[test]
    fn hybrid_stages_compose() {
        // The Hybrid variant must hold an ordered Vec so the
        // executor can iterate without re-deriving order from
        // the IR. Sanity-check the shape.
        let d = RouteDecision::Hybrid {
            stages: vec![],
            reason: "(empty hybrid — placeholder for staged composition)",
        };
        match d {
            RouteDecision::Hybrid { stages, .. } => assert!(stages.is_empty()),
            _ => panic!("expected Hybrid variant"),
        }
    }
}
