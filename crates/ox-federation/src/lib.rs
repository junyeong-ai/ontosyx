//! Ontosyx federation engine.
//!
//! `ox-federation` is the virtual-ontology-layer execution path
//! (see `docs/adr/0001-virtual-ontology-layer.md`). It owns the
//! translation from `QueryIR` / `OntologyIR` down to an Apache
//! DataFusion `LogicalPlan` that scans the original data sources
//! through the `DataSourceAdapter` trait.
//!
//! Phase 2 scope: the `TableProvider` wrapper + a `FederationContext`
//! that registers adapter-backed tables with DataFusion. Query
//! planning (path decomposition, workspace predicate injection,
//! cost estimation) lands in Phase 6; this crate exists so the wiring
//! can compose without a big-bang integration later.
//!
//! Module layout:
//!
//! - [`table_provider`] — `SourceTableProvider<A: DataSourceAdapter>`
//!   implementing DataFusion's `TableProvider` trait.
//! - [`context`] — `FederationContext` — workspace-scoped
//!   `SessionContext` factory; registers tables and runs SQL.
//! - [`error`] — federation-specific error variants that wrap
//!   `datafusion::error::DataFusionError` and `ox_core::OxError`.

#![cfg_attr(
    test,
    allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

pub mod adapter_resolver;
pub mod context;
pub mod error;
pub mod planner;
pub mod table_provider;

pub use adapter_resolver::{AdapterResolver, InMemoryAdapterResolver};
pub use context::FederationContext;
pub use error::{FederationError, FederationResult};
pub use planner::{
    ExpandedTargets, HopMappingEntry, HopSpec, InterfaceExpander, LabelResolver,
    MappingResolver, MatchPlanSpec, MatchPlanner, NodeScanSpec, ResolvedLabelTarget,
    ResolvedMappings, ScanMappingEntry, TailClauses, WorkspaceScope, build_match_op,
    build_match_plan, build_match_plan_with_projections, build_query_ir, build_query_ir_scoped,
};
pub use table_provider::SourceTableProvider;
