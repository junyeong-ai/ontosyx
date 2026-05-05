//! Graph dialect implementations.
//!
//! Each submodule packages one query-language dialect for the graph
//! runtime — its AST, parser, validator + rewriter pipeline, and
//! emit pass. Today the only landed dialect is Cypher (Neo4j +
//! Memgraph); future siblings (`gql` for ISO/IEC GQL 2024,
//! `gremlin` for TinkerPop) sit beside it without disturbing the
//! shared isolation / enrichment / profiler / registry layers, which
//! stay dialect-agnostic at the crate root.
//!
//! The runtime's `bolt::pipeline::run_pre_execute` consumes a dialect
//! through its parser + AST + render contract; adding a dialect is
//! "new module + impl + register at startup" rather than a
//! cross-cutting refactor.

pub mod cypher;
