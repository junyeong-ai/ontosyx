//! System-owned property names — single source of truth for every
//! rewriter and validator that injects, reads, or guards them.
//!
//! Adding a new system property is a one-place change here; the
//! [`SafetyValidator`] reserved-write guard, the
//! [`crate::cypher::rewrite::WorkspaceScopeRewriter`], and the
//! [`crate::cypher::soft_delete_rewriter::SoftDeleteRewriter`] all
//! consult [`SYSTEM_PROPERTIES`] so a new entry is honoured by
//! every consumer at the same time.
//!
//! Convention: leading `_` marks system ownership. Operators must
//! not write to these keys directly — the validator rejects user
//! queries that touch them.

/// Workspace-isolation tag injected by
/// [`crate::cypher::rewrite::WorkspaceScopeRewriter`] on every
/// node and edge created or matched on a non-bypass request.
pub const WORKSPACE_PROPERTY: &str = "_workspace_id";

/// Tombstone timestamp set by
/// [`crate::cypher::soft_delete_rewriter::SoftDeleteRewriter`] when
/// a `DELETE` / `DETACH DELETE` lands on a non-bypass request.
/// `IS NULL` is the read-side filter; the retention compactor
/// hard-deletes rows whose timestamp is older than the configured
/// TTL.
pub const TOMBSTONE_PROPERTY: &str = "_deleted_at";

/// Every name an operator query is forbidden from writing to via
/// `SET` / `CREATE` / `MERGE`. The
/// [`crate::cypher::validate::SafetyValidator`] consults this list
/// to reject reserved writes.
pub const SYSTEM_PROPERTIES: &[&str] = &[WORKSPACE_PROPERTY, TOMBSTONE_PROPERTY];
