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

pub mod acl_rewriter;
pub mod ast;
pub mod diagnostics;
pub mod parse;
pub mod principal;
pub mod rewrite;
pub(crate) mod rewrite_helpers;
pub mod shacl_validator;
pub mod soft_delete_rewriter;
pub mod token;
pub mod validate;

pub use acl_rewriter::{AclAction, AclPolicySpec, AclRewriter, AclSnapshot};
pub use principal::RequestPrincipal;
pub use ast::{
    ClauseKind, CypherAst, CypherClause, CypherPattern, CypherPatternElement, CypherStatement,
    NodePattern, RelDirection, RelationshipPattern, UnionKind,
};
pub use diagnostics::strict_advisory_diagnostics;
pub use parse::parse;
pub use rewrite::{
    CypherRewriter, CypherRewriterPipeline, RewriteContext, RewriteError, RewritePhase,
    RewrittenAst, WorkspaceScopeRewriter,
};
pub use shacl_validator::ShaclValidator;
pub use soft_delete_rewriter::{SoftDeleteRewriter, TOMBSTONE_PROPERTY};
pub use token::{CypherToken, Span, TokenKind, tokenize};
pub use validate::{
    ComplexityValidator, CypherValidator, CypherValidatorPipeline, IssueLevel, OntologyValidator,
    SafetyValidator, SemanticGuardValidator, ValidateContext, ValidatePhase, ValidationIssue,
    ValidationReport,
};
