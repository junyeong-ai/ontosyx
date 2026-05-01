//! Federation planner.
//!
//! Translates `QueryIR` + `OntologyIR` into a DataFusion
//! `LogicalPlan`: where the query lives in logical terms (node
//! types, labels, properties) the planner resolves those to
//! physical mappings and emits a concrete scan plan.
//!
//! Pipeline of narrow, independently-testable stages:
//!
//! | Stage | Role |
//! |-------|------|
//! | `InterfaceExpander`         | `(:IHasAddress)` → union of implementing NodeTypes |
//! | `MappingResolver`           | NodeTypeId → `ObjectMappingDef` list, precedence-sorted |
//! | `LogicalPlanBuilder`        | QueryOp → DataFusion `LogicalPlan` |
//!
//! Each stage is a pure function over its inputs — no I/O, no
//! `tokio::spawn`, no hidden state — so callers can compose custom
//! pipelines (e.g. a preview / dry-run planner that stops before
//! execution) without fighting shared mutable state.

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
