# PlanRouter — Cypher / Federation / Hybrid dispatch

**Status:** Design sketch — Phase 6 of the long-horizon work plan.
The trait + integration points are documented here so the next
session can land the implementation without re-deriving the
contract.

## Problem

`query_graph` (the agent's NL-to-result tool) calls
`runtime.execute_query(...)` directly today. That hardcodes
the Cypher path: every translated `QueryIR` lowers through the
graph compiler and runs against the bolt driver, regardless of
whether the data actually lives in the graph.

Two consequences:

- **`ox-federation` is unreachable from the agent.** The
  DataFusion VOL planner exists (`ox-federation::FederationContext::execute_plan`)
  but no agent path invokes it. Cross-source SQL queries that
  *should* lower to Arrow execution cannot reach federation
  through the platform's primary tool.
- **The wrong runtime gets the wrong query shape.** A pure
  one-table aggregate over a mapped Postgres relation
  (`SELECT region, sum(amount) FROM orders GROUP BY region`)
  round-trips through Neo4j; a 4-hop traversal over densely-
  connected graph data can land in a federated DataFusion
  plan that materialises every endpoint into Arrow.

## Decision (sketch)

A `PlanRouter` trait sits between the Brain's translation
output and the runtime invocation. Inputs: `QueryIR` +
`OntologyIR`. Output: a `RouteDecision` the agent dispatches
on.

```rust
#[async_trait]
pub trait PlanRouter: Send + Sync {
    /// Decide which execution path the QueryIR should take.
    ///
    /// Implementations inspect the IR's pattern depth, the
    /// ontology's mapping density on the touched node types,
    /// and any caller-supplied cost-budget hints; they MUST
    /// be deterministic for the same `(QueryIR, OntologyIR)`
    /// pair so plan-cache keys stay stable.
    async fn route(
        &self,
        ir: &QueryIR,
        ontology: &OntologyIR,
        budget: Option<&CostBudget>,
    ) -> OxResult<RouteDecision>;
}

pub enum RouteDecision {
    /// Single-runtime Cypher execution. The agent calls
    /// `runtime.execute_query(...)` as today; the router's
    /// only contribution is the `reason` (an attribution
    /// string the FE renders alongside the result so the
    /// operator sees *why* the platform picked this path).
    Cypher { reason: &'static str },

    /// Single-runtime DataFusion execution. The agent
    /// calls `FederationContext::execute_plan(...)` with
    /// the supplied logical plan.
    Federation {
        plan: datafusion::logical_expr::LogicalPlan,
        reason: &'static str,
    },

    /// Multi-runtime composition. Stages execute in order;
    /// each stage's output feeds the next stage's input.
    /// Future-shape — the v1 router emits this only for
    /// queries whose IR genuinely benefits (a graph
    /// traversal that ends in a federated aggregate).
    Hybrid {
        stages: Vec<HybridStage>,
        reason: &'static str,
    },
}

pub enum HybridStage {
    Cypher { ir: QueryIR },
    Federation { plan: datafusion::logical_expr::LogicalPlan },
}

pub struct CostBudget {
    pub max_rows: Option<u64>,        // auto-LIMIT injection threshold
    pub max_wallclock_ms: Option<u64>,
    pub allow_high_cost: bool,        // override for `RiskLevel::High`
}
```

## Routing heuristics (v1)

The first-cut router uses three signals — coarse on purpose;
the cost model lives in a follow-up:

1. **Mapping density.** If every node type touched by the
   IR's pattern has a `ObjectMappingDef` to the same
   physical source, prefer `Federation { ... }` — the
   federation planner can lower the whole pattern to a SQL
   join without round-tripping through Neo4j.

2. **Pattern depth.** Single-hop or zero-hop patterns over
   mapped data → `Federation`. Patterns of depth ≥ 2 over
   the cached graph → `Cypher`. Patterns mixing mapped +
   cached endpoints → `Hybrid` (or `Federation` if the
   federation planner already supports the cross-source
   join through a `Bridge` link mapping).

3. **Cost-budget overrides.** `CostBudget.allow_high_cost ==
   false` + `cost::estimate_cost(ir, ontology) ==
   RiskLevel::High` → reject. The router returns
   `OxError::Validation` rather than a `RouteDecision`,
   surfacing the typed `ApiErrorCode::QueryCostBudgetExceeded`
   error code (per ADR-0017's typed-error contract).

## Integration points

- **Brain output stays unchanged.** `translate_query` returns
  `QueryIR` as today; the router runs *after* translation.
  No Brain trait surface change.
- **Agent dispatch.** `query_graph::handle` swaps the direct
  `runtime.execute_query(...)` call for a router invocation
  + a match on `RouteDecision`. Every existing FE consumer
  of the result shape stays compatible because both
  Cypher and Federation paths land their results in the
  same `QueryExecution` row.
- **Provenance.** Each `RouteDecision` carries a `reason`
  string that lands in `QueryExecution.metadata.routing` so
  the FE result panel can render "this query went through
  the federation planner because every endpoint was
  Postgres-mapped" alongside the result.
- **Plan cache.** Cypher path keeps the existing
  compile-plan cache; federation path piggybacks on
  DataFusion's logical-plan caching. Both are keyed on the
  ontology version + IR hash, so cache pollution across a
  schema change is impossible.

## Out of scope (v1)

- **Cost-model accuracy.** v1 uses the existing
  `cost::estimate_cost` (`Low / Medium / High`); the
  cost-aware fallback (a follow-up plan) replaces this with
  cardinality estimates from `AdapterCapabilities` +
  `MappingResolver` selectivity hints.
- **Query plan splitting (DataFusion → graph).** A query
  whose result feeds a graph traversal can land as
  `Hybrid { stages: [Federation, Cypher] }` once the
  intermediate-batch handoff is implemented.
- **Adaptive routing.** v1 is deterministic — same input
  always picks the same decision. An adaptive router that
  re-routes on observed slow runs is a Phase 9-class
  observability follow-up.

## Test pyramid

- **Unit tests on the router** — `(QueryIR, OntologyIR)` →
  expected `RouteDecision`. Pin the heuristic at the IR
  level so the router refactors don't drift.
- **Integration tests in `ox-api/tests/`** — fire questions
  through the agent's `query_graph` tool, assert the
  `RouteDecision` lands on the persisted execution row's
  `metadata.routing`. The `tests/golden/nl2cypher.golden.json`
  dataset already exists; the integration tests compose
  the questions with the routing assertion.
- **End-to-end** — `bash scripts/e2e-korean.sh` should run
  at least one question per route variant.

## References

- ADR-0001 — Virtual Ontology Layer.
- ADR-0002 — DataFusion as federation engine.
- ADR-0017 — Typed error wire shape (`QueryCostBudgetExceeded`).
- ADR-0018 — `EvaluationStore` (the routing decision lands
  on `evaluation_cases.metadata` for the eval pipeline).
- `tests/golden/nl2cypher.golden.json` — the eval dataset
  the routing decision needs to satisfy.
- Memory entry: revised work-plan Phase 6 — names the
  PlanRouter as the single biggest competitive moat
  (currently NL → Federation is unreachable, so the
  ox-federation crate's value is dark).
