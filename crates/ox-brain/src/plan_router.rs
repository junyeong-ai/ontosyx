//! `PlanRouter` — execution-backend dispatch above the runtime layer.
//!
//! Every NL → QueryIR translation eventually needs an execution
//! backend. The platform supports two and a composition of both:
//! the [`GraphRuntime`](ox_graph_runtime::GraphRuntime) (graph-native
//! pattern execution against Neo4j / Memgraph) and the
//! [`FederationContext`](ox_federation::FederationContext)
//! (DataFusion logical-plan execution across heterogeneous sources).
//! `PlanRouter` is the dispatcher that decides which one — or which
//! staged combination — should run a given IR.
//!
//! Implementations are stateless and deterministic; the same
//! `(QueryIR, OntologyIR)` pair must produce the same
//! [`RouteDecision`] so plan-cache keys stay stable across calls.

use async_trait::async_trait;

use ox_core::error::OxResult;
use ox_ontology::ir::OntologyIR;
use ox_query_ir::query::QueryIR;

/// Decide which execution backend a `QueryIR` should run on.
#[async_trait]
pub trait PlanRouter: Send + Sync {
    /// Resolve a routing decision for the supplied IR.
    ///
    /// `budget == None` means "use the workspace-default cost
    /// ceiling"; the implementation reads the workspace config to
    /// pick the actual numbers.
    async fn route(
        &self,
        ir: &QueryIR,
        ontology: &OntologyIR,
        budget: Option<&CostBudget>,
    ) -> OxResult<RouteDecision>;
}

/// Where the platform should execute the IR.
///
/// `Graph` and `Federation` are leaf decisions; `Hybrid` composes
/// both backends in a fixed staged order so a graph-traversal-then-
/// federated-aggregate query stops round-tripping through one
/// runtime that's wrong for half the work.
#[derive(Debug, Clone)]
pub enum RouteDecision {
    /// Execute on the `GraphRuntime` (Cypher emitted by
    /// `ox-compiler`, executed against Neo4j / Memgraph).
    Graph {
        /// Stable, FE-renderable explanation of the routing
        /// choice. Carried on `QueryExecution.metadata.routing` so
        /// the result panel surfaces it without re-deriving.
        reason: &'static str,
    },

    /// Execute on the `FederationContext` (DataFusion logical
    /// plan, per-source data lives in its own adapter).
    Federation {
        /// Pre-built DataFusion logical plan keyed against the
        /// IR. Stored as `serde_json::Value` rather than a
        /// concrete `datafusion::logical_expr::LogicalPlan` so
        /// this crate stays free of the DataFusion dep —
        /// `ox-federation` is the only crate that links against
        /// it. The federation context deserialises + optimises
        /// before execution.
        plan: serde_json::Value,
        /// FE-renderable explanation, same shape as `Graph`.
        reason: &'static str,
    },

    /// Multi-backend composition. Stages execute in order; each
    /// stage's output feeds the next stage's input. Emit only
    /// when the IR genuinely benefits — a graph traversal whose
    /// result set feeds a federated aggregate, for example.
    Hybrid {
        /// Sequence of single-backend stages, executed in order.
        /// Stage N reads stage N-1's output as a bound table.
        stages: Vec<HybridStage>,
        reason: &'static str,
    },
}

/// One stage in a `Hybrid` route. Mirrors the leaf variants of
/// [`RouteDecision`] one-for-one.
#[derive(Debug, Clone)]
pub enum HybridStage {
    /// A graph-runtime sub-query whose result rows feed the next
    /// stage's bound input.
    Graph { ir: QueryIR },
    /// A DataFusion sub-plan whose result rows feed the next
    /// stage's bound input.
    Federation { plan: serde_json::Value },
}

/// Caller-supplied cost ceiling. Routers refuse the query before
/// execution when the IR's cost estimate exceeds the budget unless
/// the caller opts into high-cost runs.
#[derive(Debug, Clone, Default)]
pub struct CostBudget {
    /// Auto-`LIMIT` injection threshold. `None` means "no cap on
    /// row count" — the router still honours `allow_high_cost`.
    pub max_rows: Option<u64>,
    /// Wallclock cap. The runtime layer enforces; the router
    /// refuses to dispatch when the IR's estimated time exceeds
    /// this without `allow_high_cost`.
    pub max_wallclock_ms: Option<u64>,
    /// Override for `cost::estimate_cost == RiskLevel::High`.
    /// Default `false`: high-risk queries refuse with a typed
    /// `ApiErrorCode::QueryCostBudgetExceeded`.
    pub allow_high_cost: bool,
}

/// Default `PlanRouter` impl. Returns [`RouteDecision::Graph`]
/// uniformly — every QueryIR routes through the graph runtime
/// regardless of pattern shape, ontology mapping density, or
/// caller-supplied budget. Stateless; one instance per process.
#[derive(Debug, Default, Clone, Copy)]
pub struct DefaultPlanRouter;

impl DefaultPlanRouter {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl PlanRouter for DefaultPlanRouter {
    async fn route(
        &self,
        _ir: &QueryIR,
        _ontology: &OntologyIR,
        _budget: Option<&CostBudget>,
    ) -> OxResult<RouteDecision> {
        Ok(RouteDecision::Graph {
            reason: "graph runtime",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_inputs() -> (QueryIR, OntologyIR) {
        let ir = QueryIR {
            schema_version: ox_query_ir::query::QUERY_IR_SCHEMA_VERSION,
            operation: ox_query_ir::query::QueryOp::Match {
                patterns: vec![],
                filter: None,
                projections: vec![],
                optional: false,
                group_by: vec![],
            },
            limit: None,
            skip: None,
            order_by: vec![],
            as_of: None,
        };
        let ontology = OntologyIR::new(
            "test-ont".into(),
            "test".into(),
            ox_core::i18n::LocalizedText::default(),
            ox_ontology::ir::OntologyVersion::from(1u32),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );
        (ir, ontology)
    }

    #[test]
    fn cost_budget_default_refuses_high() {
        let b = CostBudget::default();
        assert!(!b.allow_high_cost);
        assert!(b.max_rows.is_none());
        assert!(b.max_wallclock_ms.is_none());
    }

    #[test]
    fn route_decision_graph_carries_reason() {
        let d = RouteDecision::Graph {
            reason: "graph-native traversal",
        };
        match d {
            RouteDecision::Graph { reason } => assert_eq!(reason, "graph-native traversal"),
            _ => panic!("expected Graph variant"),
        }
    }

    #[test]
    fn hybrid_stages_compose_in_order() {
        let d = RouteDecision::Hybrid {
            stages: vec![],
            reason: "staged composition",
        };
        match d {
            RouteDecision::Hybrid { stages, .. } => assert!(stages.is_empty()),
            _ => panic!("expected Hybrid variant"),
        }
    }

    #[tokio::test]
    async fn default_router_emits_graph_with_static_reason() {
        let router = DefaultPlanRouter::new();
        let (ir, ontology) = empty_inputs();

        let decision = router.route(&ir, &ontology, None).await.expect("route");
        match decision {
            RouteDecision::Graph { reason } => assert_eq!(reason, "graph runtime"),
            other => panic!("expected Graph routing, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn default_router_ignores_cost_budget() {
        let router = DefaultPlanRouter::new();
        let (ir, ontology) = empty_inputs();
        let budget = CostBudget {
            max_rows: Some(0),
            max_wallclock_ms: Some(0),
            allow_high_cost: false,
        };

        let decision = router
            .route(&ir, &ontology, Some(&budget))
            .await
            .expect("route");
        assert!(matches!(decision, RouteDecision::Graph { .. }));
    }
}
