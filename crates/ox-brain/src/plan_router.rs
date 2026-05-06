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
//! `(QueryIR, OntologyIR, CostBudget)` triple must produce the same
//! [`RouteDecision`] so plan-cache keys stay stable across calls.

use std::collections::HashSet;

use async_trait::async_trait;

use ox_compiler::cost::{QueryCost, RiskLevel, estimate_cost};
use ox_core::error::{OxError, OxResult};
use ox_ontology::ir::OntologyIR;
use ox_query_ir::query::{ChainStep, GraphPattern, QueryIR, QueryOp};

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
        /// Cost estimate that drove the decision — surfaced on the
        /// FE result panel and persisted onto
        /// `QueryExecution.metadata` for calibration. `None` only
        /// for synthetic test fixtures that bypass the production
        /// flow.
        cost: Option<QueryCost>,
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
        cost: Option<QueryCost>,
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
        cost: Option<QueryCost>,
    },
}

impl RouteDecision {
    /// Operator-renderable reason regardless of variant.
    pub fn reason(&self) -> &'static str {
        match self {
            Self::Graph { reason, .. }
            | Self::Federation { reason, .. }
            | Self::Hybrid { reason, .. } => reason,
        }
    }

    /// Cost estimate accessor — present when the router consulted
    /// `estimate_cost`; `None` for unit-test stubs and
    /// federation-pre-built routes that bypass the cost step.
    pub fn cost(&self) -> Option<&QueryCost> {
        match self {
            Self::Graph { cost, .. }
            | Self::Federation { cost, .. }
            | Self::Hybrid { cost, .. } => cost.as_ref(),
        }
    }
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
///
/// The router consults each axis independently:
///   - `max_rows` is checked against `QueryCost.estimated_rows`
///     when it is `Some` (today only available when the cost path
///     consumes `ColumnProfile.row_count`; until then this is a
///     no-op for the row-count axis).
///   - `max_wallclock_ms` is checked against
///     `QueryCost.estimated_wallclock_ms`.
///   - `RiskLevel::High` is rejected unconditionally unless
///     `allow_high_cost == true`.
#[derive(Debug, Clone, Default)]
pub struct CostBudget {
    /// Estimated-row cap. `None` means "no cap on row count" — the
    /// router still honours `allow_high_cost`.
    pub max_rows: Option<u64>,
    /// Wallclock cap in milliseconds. The router refuses to
    /// dispatch when `cost.estimated_wallclock_ms` exceeds this.
    pub max_wallclock_ms: Option<u64>,
    /// Override for `RiskLevel::High`. Default `false`: high-risk
    /// queries refuse with a typed `ApiErrorCode::QueryCostBudgetExceeded`.
    pub allow_high_cost: bool,
}

/// Why the router rejected a route. Carried to `ox-api` so the
/// HTTP layer can lift it to a typed `QueryCostBudgetExceeded`
/// 422 with the matching params.
///
/// Internal use only — `OxError::Validation` carries the wire
/// shape; the api crate maps `[budget]` prefix to the typed code.
fn reject_with_budget(
    cost: &QueryCost,
    detail: impl Into<String>,
) -> OxError {
    // The cost estimator's RiskLevel + extrapolation lands in the
    // detail message + structured-error params via ox-api. Encode
    // the params inline so `AppError::query_cost_budget_exceeded`
    // can pull them by parsing once.
    let detail = detail.into();
    OxError::Validation {
        field: "query_ir".to_string(),
        message: format!(
            "[budget] risk={:?} expansions={} wallclock_ms={} :: {}",
            cost.risk_level,
            cost.estimated_pattern_expansions,
            cost.estimated_wallclock_ms
                .map(|n| n.to_string())
                .unwrap_or_else(|| "unknown".to_string()),
            detail
        ),
    }
}

/// Heuristic `PlanRouter` — production dispatch for the platform.
///
/// Decision tree:
///   1. Compute `QueryCost` via `ox_compiler::cost::estimate_cost`.
///   2. Reject when `cost.risk_level == High && !budget.allow_high_cost`.
///   3. Reject when `budget.max_wallclock_ms` is set and
///      `cost.estimated_wallclock_ms` exceeds it.
///   4. Reject when `budget.max_rows` is set and
///      `cost.estimated_rows` exceeds it (today `estimated_rows`
///      is `None` so this branch is dormant; reserved for the
///      ColumnProfile-aware extension).
///   5. Detect cross-source traversal via
///      `LinkMappingDef::crosses_sources()` on every relationship
///      label the IR touches. When any edge crosses sources →
///      `RouteDecision::Federation` (today `plan` is rendered as
///      `null`; a future `ox-federation::build_match_plan` call
///      site populates the plan). Until that wiring lands,
///      cross-source detection still fires and surfaces in the
///      attribution string.
///   6. Otherwise → `RouteDecision::Graph`.
///
/// Stateless. One instance per process is sufficient.
#[derive(Debug, Default, Clone, Copy)]
pub struct HeuristicPlanRouter;

impl HeuristicPlanRouter {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl PlanRouter for HeuristicPlanRouter {
    async fn route(
        &self,
        ir: &QueryIR,
        ontology: &OntologyIR,
        budget: Option<&CostBudget>,
    ) -> OxResult<RouteDecision> {
        let cost = estimate_cost(ir, ontology);

        // 2. RiskLevel::High gate.
        if matches!(cost.risk_level, RiskLevel::High) {
            let allow = budget.map(|b| b.allow_high_cost).unwrap_or(false);
            if !allow {
                let detail = cost
                    .warnings
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "high-risk query shape".to_string());
                return Err(reject_with_budget(&cost, detail));
            }
        }

        // 3. Wallclock gate.
        if let (Some(b), Some(estimated_ms)) =
            (budget, cost.estimated_wallclock_ms)
            && let Some(cap_ms) = b.max_wallclock_ms
            && estimated_ms > cap_ms
            && !b.allow_high_cost
        {
            return Err(reject_with_budget(
                &cost,
                format!(
                    "estimated {estimated_ms} ms exceeds workspace wallclock cap of {cap_ms} ms"
                ),
            ));
        }

        // 4. Row-count gate (dormant until ColumnProfile wiring).
        if let (Some(b), Some(estimated_rows)) = (budget, cost.estimated_rows)
            && let Some(cap_rows) = b.max_rows
            && estimated_rows > cap_rows
            && !b.allow_high_cost
        {
            return Err(reject_with_budget(
                &cost,
                format!(
                    "estimated {estimated_rows} rows exceeds workspace row cap of {cap_rows}"
                ),
            ));
        }

        // 5. Federation detection.
        if traverses_cross_source_edge(ir, ontology) {
            return Ok(RouteDecision::Federation {
                plan: serde_json::Value::Null,
                reason: "federation runtime — cross-source traversal detected",
                cost: Some(cost),
            });
        }

        // 6. Default leaf.
        Ok(RouteDecision::Graph {
            reason: "graph runtime — single-source pattern execution",
            cost: Some(cost),
        })
    }
}

/// Walk a QueryIR and report `true` when any traversed
/// relationship label maps to a `LinkMappingDef` whose
/// `crosses_sources()` is true. Conservative — when a label has
/// no link mapping (LLM-hallucinated; ontology validator catches
/// it elsewhere) the walk skips silently.
fn traverses_cross_source_edge(ir: &QueryIR, ontology: &OntologyIR) -> bool {
    let mut labels: HashSet<&str> = HashSet::new();
    collect_relationship_labels(&ir.operation, &mut labels);
    labels.iter().any(|label| {
        ontology
            .link_mappings()
            .iter()
            .filter(|lm| {
                ontology
                    .edge_types()
                    .iter()
                    .any(|e| e.id == lm.edge_type_id && e.label.as_str() == *label)
            })
            .any(|lm| lm.crosses_sources())
    })
}

fn collect_relationship_labels<'a>(op: &'a QueryOp, out: &mut HashSet<&'a str>) {
    match op {
        QueryOp::Match { patterns, .. } => {
            for p in patterns {
                if let GraphPattern::Relationship {
                    label: Some(label), ..
                } = p
                {
                    out.insert(label.as_str());
                }
            }
        }
        QueryOp::PathFind { edge_types, .. } => {
            for label in edge_types {
                out.insert(label.as_str());
            }
        }
        QueryOp::Aggregate { source, .. } => {
            collect_relationship_labels(&source.operation, out);
        }
        QueryOp::Chain { steps } => {
            for step in steps {
                let ChainStep { operation, .. } = step;
                collect_relationship_labels(operation, out);
            }
        }
        QueryOp::Union { queries, .. } => {
            for q in queries {
                collect_relationship_labels(&q.operation, out);
            }
        }
        QueryOp::Mutate { context, .. } => {
            if let Some(ctx) = context {
                collect_relationship_labels(ctx, out);
            }
        }
        QueryOp::CallSubquery { inner, .. } => {
            collect_relationship_labels(&inner.operation, out);
        }
        QueryOp::Analytics { .. } => {}
        QueryOp::HybridSearch { request } => {
            // The optional graph constraint sub-pattern can pin
            // edge labels — surface them so the federation
            // detector counts hybrid-with-cross-source as
            // federation-eligible.
            if let Some(constraint) = &request.graph_constraints {
                for edge in &constraint.edges {
                    if let Some(label) = &edge.label {
                        out.insert(label.as_str());
                    }
                }
            }
        }
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
            cost: None,
        };
        assert_eq!(d.reason(), "graph-native traversal");
    }

    #[test]
    fn hybrid_stages_compose_in_order() {
        let d = RouteDecision::Hybrid {
            stages: vec![],
            reason: "staged composition",
            cost: None,
        };
        match d {
            RouteDecision::Hybrid { stages, .. } => assert!(stages.is_empty()),
            _ => panic!("expected Hybrid variant"),
        }
    }

    #[tokio::test]
    async fn heuristic_router_emits_graph_for_low_risk_single_source() {
        let router = HeuristicPlanRouter::new();
        let (ir, ontology) = empty_inputs();

        let decision = router.route(&ir, &ontology, None).await.expect("route");
        match decision {
            RouteDecision::Graph { reason, cost } => {
                assert_eq!(reason, "graph runtime — single-source pattern execution");
                let c = cost.expect("cost present");
                assert_eq!(c.pattern_count, 0);
            }
            other => panic!("expected Graph routing, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn heuristic_router_rejects_high_risk_without_allow() {
        // Synthesize a high-risk IR — unbounded var-length path
        // (Cypher `*`) is the canonical RiskLevel::High shape.
        use ox_core::graph_label::GraphLabel;
        use ox_core::variable_name::VariableName;
        use ox_core::types::Direction;
        use ox_query_ir::query::{GraphPattern as Gp, QueryOp, VarLength};

        let ir = QueryIR {
            schema_version: ox_query_ir::query::QUERY_IR_SCHEMA_VERSION,
            operation: QueryOp::Match {
                patterns: vec![
                    Gp::Node {
                        variable: VariableName::new("a").unwrap(),
                        label: Some(GraphLabel::new("A").unwrap()),
                        property_filters: vec![],
                    },
                    Gp::Node {
                        variable: VariableName::new("b").unwrap(),
                        label: Some(GraphLabel::new("B").unwrap()),
                        property_filters: vec![],
                    },
                    Gp::Relationship {
                        variable: None,
                        label: Some(GraphLabel::new("REL").unwrap()),
                        source: VariableName::new("a").unwrap(),
                        target: VariableName::new("b").unwrap(),
                        direction: Direction::Outgoing,
                        property_filters: vec![],
                        var_length: Some(VarLength {
                            min: Some(1),
                            max: None,
                        }),
                    },
                ],
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
            "test".into(),
            "t".into(),
            ox_core::i18n::LocalizedText::default(),
            ox_ontology::ir::OntologyVersion::from(1u32),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );

        let router = HeuristicPlanRouter::new();
        // Default budget refuses High.
        let err = router
            .route(&ir, &ontology, None)
            .await
            .expect_err("high-risk should reject without allow_high_cost");
        let msg = format!("{err:?}");
        assert!(msg.contains("[budget]"), "got {msg}");
    }

    #[tokio::test]
    async fn heuristic_router_passes_high_risk_when_explicitly_allowed() {
        use ox_core::graph_label::GraphLabel;
        use ox_core::variable_name::VariableName;
        use ox_query_ir::query::{GraphPattern as Gp, QueryOp};

        let ir = QueryIR {
            schema_version: ox_query_ir::query::QUERY_IR_SCHEMA_VERSION,
            operation: QueryOp::Match {
                // Cartesian: two unconnected node patterns.
                patterns: vec![
                    Gp::Node {
                        variable: VariableName::new("a").unwrap(),
                        label: Some(GraphLabel::new("A").unwrap()),
                        property_filters: vec![],
                    },
                    Gp::Node {
                        variable: VariableName::new("b").unwrap(),
                        label: Some(GraphLabel::new("B").unwrap()),
                        property_filters: vec![],
                    },
                ],
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
            "t".into(),
            "t".into(),
            ox_core::i18n::LocalizedText::default(),
            ox_ontology::ir::OntologyVersion::from(1u32),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );

        let router = HeuristicPlanRouter::new();
        let budget = CostBudget {
            allow_high_cost: true,
            ..Default::default()
        };
        let decision = router
            .route(&ir, &ontology, Some(&budget))
            .await
            .expect("opt-in passes");
        // Cartesian shape with no cross-source edges still routes
        // to Graph — the budget gate cleared, cross-source check
        // returned false.
        assert!(matches!(decision, RouteDecision::Graph { .. }));
    }
}
