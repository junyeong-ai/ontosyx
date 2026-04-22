//! `ox-query-ir` — DB-agnostic compile target for Ontosyx queries.
//!
//! Phase 3-B (2026-04-20): `query_ir`, `pattern_ir`,
//! `structured_match_query`, `query_bindings` migrated here wholesale
//! from `ox-core`, renamed to `query`, `pattern`, `structured_match`,
//! `bindings`. The 1,555-line `query.rs` will split further into
//! domain submodules (`op`, `expr`, `projection`, `mutate`, ...) in
//! Phase 3-C.

#![cfg_attr(
    test,
    allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

pub mod bindings;
pub mod eval;
pub mod ontology_conformance;
pub mod pattern;
pub mod query;
pub mod structured_match;

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
