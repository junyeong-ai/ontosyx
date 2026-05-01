//! `ox-query-ir` — DB-agnostic compile target for Ontosyx queries.
//!
//! Two cooperating IRs: `QueryIR` (the compile target every
//! downstream consumer works against) and `PatternIR` (the canvas-
//! ergonomic UI form). They round-trip via `compile / decompile`.

#![cfg_attr(
    test,
    allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

pub mod bindings;
pub mod eval;
pub mod insight;
pub mod ontology_conformance;
pub mod pattern;
pub mod query;
pub mod structured_match;

pub use insight::{InsightDef, InsightId};
pub use ontology_conformance::unknown_labels_in_query;

// ---------------------------------------------------------------------------
// Re-exports — preserve the v1 call-surface (`ox_query_ir::QueryIR` etc.)
// ---------------------------------------------------------------------------

pub use bindings::{
    BindingKind, EdgeBinding, NodeBinding, PropertyBinding, ResolvedQueryBindings,
    resolve_query_bindings,
};
pub use pattern::{
    LayoutHints, PatternEdge, PatternFilter, PatternIR, PatternNode, PatternProjection, Position,
};
pub use query::QueryIR;
pub use structured_match::StructuredMatchQuery;
