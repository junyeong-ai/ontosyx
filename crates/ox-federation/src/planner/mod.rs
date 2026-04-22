//! Federation planner.
//!
//! The planner translates a `QueryIR` + `OntologyIR` snapshot into a
//! DataFusion `LogicalPlan` the engine can execute. It is the
//! centre-piece of the Virtual Ontology Layer (ADR 0001): where the
//! query lives in logical terms (node types, labels, properties) the
//! planner resolves those terms to physical mappings and emits a
//! concrete scan plan.
//!
//! The pipeline composes of narrow, independently-testable stages.
//! Phase 6-A ships two of them — the rest are filled in as the
//! planner's workload grows:
//!
//! | Stage | Role | Status |
//! |-------|------|--------|
//! | `OntologyResolver`          | Pick the OntologyIR version for a given `ontology_valid_at` | Phase 6-B |
//! | `InterfaceExpander`         | `(:IHasAddress)` → union of implementing NodeTypes         | **Phase 6-A** |
//! | `TemporalRewriter`          | Rewrite renamed labels current→snapshot                     | Lives in ox-compiler today (moves here in Phase 6-B) |
//! | `MappingResolver`           | NodeTypeId → ObjectMappingDef list, precedence-sorted       | **Phase 6-A** |
//! | `WorkspacePredicateInjector`| Push `_workspace_id = $_ws_id` into every scan              | Phase 6-B |
//! | `RulePreValidator`          | SHACL Core precondition checks on mutations                 | Phase 6-B |
//! | `PathDecomposer`            | Variable-length path → recursive CTE or fixed-k expansion  | Phase 6-C |
//! | `LogicalPlanBuilder`        | QueryOp → DataFusion `LogicalPlan`                          | Phase 6-C |
//! | `CostEstimator`             | Annotate plan with per-source cost hints                    | Phase 6-D |
//! | `PhysicalOptimizer`         | Bloom-join hints, projection/predicate pushdown polish      | Phase 6-D |
//! | `Dispatcher`                | Route each scan to source vs. graph cache                   | Phase 6-E |
//! | `Executor`                  | Drive the `SessionContext` and collect batches              | Phase 6-E |
//! | `ProvenanceTagger`          | Attach `ProvenanceDef` to emitted rows                      | Phase 6-F |
//! | `ResultShaper`              | Final result shape + `PartialFailureKind` handling          | Phase 6-F |
//!
//! Each stage is a pure function over its inputs — no I/O, no
//! `tokio::spawn`, no hidden state. That keeps them trivially
//! testable and lets future callers compose custom pipelines (e.g. a
//! preview / dry-run planner that stops before execution) without
//! fighting shared mutable state.

pub mod expr_lowering;
pub mod interface_expander;
pub mod label_resolver;
pub mod logical_plan_builder;
pub mod mapping_resolver;
pub mod match_planner;

pub use interface_expander::{ExpandedTargets, InterfaceExpander};
pub use label_resolver::{LabelResolver, ResolvedLabelTarget};
pub use logical_plan_builder::{
    TailClauses, WorkspaceScope, build_match_op, build_match_plan,
    build_match_plan_with_projections, build_query_ir, build_query_ir_scoped,
};
pub use mapping_resolver::{MappingResolver, ResolvedMappings};
pub use match_planner::{
    HopMappingEntry, HopSpec, MatchPlanSpec, MatchPlanner, NodeScanSpec, ScanMappingEntry,
};
