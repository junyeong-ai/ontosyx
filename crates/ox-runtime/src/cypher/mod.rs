//! Cypher processing pipeline.
//!
//! The submodules here replace the ad-hoc string-manipulation approach
//! that `scope_cypher` used previously. A unified partial AST plus
//! rewriter and validator pipelines become the shared infrastructure for
//! every cross-cutting Cypher concern.
//!
//! Workspace isolation is the first client of this pipeline — it lands
//! as `WorkspaceScopeRewriter` in a follow-up commit, replacing the
//! string-based `scope_cypher`. Future passes target ACL row-level
//! filtering, temporal `as_of` queries, soft-delete tombstone filters,
//! and a `CypherValidator` safety + ontology conformance gate for
//! LLM-generated queries before execution.
//!
//! Design goals:
//!
//! 1. **Lossless round trip.** Parsing and rendering must reproduce the
//!    original source exactly for any AST the parser returns — formatting,
//!    comments, unknown constructs.
//! 2. **Progressive refinement.** The AST only structures what passes need
//!    (clause boundaries, patterns). Everything else survives as raw text
//!    and preserved tokens, so future passes can drill deeper without
//!    invalidating existing ones.
//! 3. **Composable rewrites.** Rewriters see an AST, emit an AST; the
//!    pipeline handles ordering and conflicts. A new cross-cutting concern
//!    is a new `impl CypherRewriter`, not a new ad-hoc string function.

pub mod ast;
pub mod parse;
pub mod rewrite;
pub mod token;

pub use ast::{
    CypherAst, CypherClause, CypherPattern, CypherPatternElement, CypherStatement, ClauseKind,
    NodePattern, RelDirection, RelationshipPattern, UnionKind,
};
pub use parse::parse;
pub use rewrite::{
    CypherRewriter, CypherRewriterPipeline, RewriteContext, WorkspaceScopeRewriter,
};
pub use token::{CypherToken, Span, TokenKind, tokenize};
