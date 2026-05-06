//! `ActionExecutor` — typed write surface for ontology mutations.
//!
//! Symmetric to [`crate::GraphRuntime`] for reads:
//! `GraphRuntime::execute_query` runs a compiled `QueryIR` against
//! the graph backend, `ActionExecutor::invoke_action` runs an
//! `ActionDef` against the same backend (or a federated source,
//! depending on the action's body). Both honour the
//! `GRAPH_ONTOLOGY` task-local for context.
//!
//! # Approval flow
//!
//! `ActionDef.approval_policy` declares the gate. The executor
//! honours it without the caller knowing:
//!
//! - [`ApprovalPolicy::Automatic`] → executes immediately, returns
//!   [`ActionResult::Executed`].
//! - [`ApprovalPolicy::RequireApproval`] → never executes
//!   directly; lands a `HeuristicProposal` row (per ADR-0023's
//!   no-auto-decisions invariant) and returns
//!   [`ActionResult::PendingApproval`]. The governance approval
//!   surface picks up the proposal; on approve, the approval
//!   handler re-invokes `ActionExecutor::invoke_action` with the
//!   approval id threaded so the executor recognises the
//!   post-approval invocation and lands as
//!   [`ActionResult::Executed`].

use async_trait::async_trait;

use ox_core::error::OxResult;
use ox_ontology::action::ActionId;
use ox_ontology::provenance::{EntityRef, ProvenanceId};

/// Invoke a typed action against the platform.
///
/// The implementation:
///
/// 1. Resolves `action_id` against the active `OntologyIR::actions()`
///    (per the `GRAPH_ONTOLOGY` task-local).
/// 2. Validates `params.values` against the action's
///    `parameters: Vec<ActionParameter>` schema.
/// 3. Evaluates the action's `preconditions` against the bound
///    subject.
/// 4. Honours the action's `approval_policy` — `RequireApproval`
///    routes through the [`HeuristicProposal`] queue (per ADR-0023)
///    and returns [`ActionResult::PendingApproval`] without
///    executing.
/// 5. Executes the action's body — a typed graph / federation /
///    function write operation declared on the `ActionDef`.
/// 6. Evaluates the action's `postconditions` inside the same
///    transaction — failure rolls back.
/// 7. Emits a `prov:Activity` row (per ADR-0008's PROV-O contract;
///    activity kind `ActionExecute { action_id, idempotency_key }`).
#[async_trait]
pub trait ActionExecutor: Send + Sync {
    /// Resolve + validate + execute (or queue) the action.
    async fn invoke_action(
        &self,
        action_id: &ActionId,
        params: &ActionInvocationParams,
        principal: &Principal,
    ) -> OxResult<ActionResult>;
}

/// Result of an `invoke_action` call. Three variants matching the
/// three approval-policy outcomes:
///
/// - [`Executed`] — the action ran. `affected` carries the typed
///   counters; `provenance_id` links to the PROV-O record for
///   "what happened" drilldown.
/// - [`PendingApproval`] — the action's approval policy is
///   `RequireApproval` and the request is durable in the
///   `HeuristicProposal` queue.
/// - [`DryRun`] — the planner produced the would-be effects
///   without executing.
#[derive(Debug, Clone, PartialEq)]
pub enum ActionResult {
    Executed {
        affected: ActionAffected,
        provenance_id: ProvenanceId,
    },
    PendingApproval {
        /// String id of the `HeuristicProposal` row carrying the
        /// queued invocation. Stored as `String` rather than a
        /// typed `HeuristicProposalId` so this crate stays free
        /// of an `ox-store` dep — `ox-api` resolves the id when
        /// wiring the approval handler.
        proposal_id: String,
    },
    DryRun {
        affected: ActionAffected,
    },
}

/// Parameters for `invoke_action`. Carries the subject, the
/// caller-supplied parameter values, the idempotency key, and the
/// dry-run flag.
#[derive(Debug, Clone)]
pub struct ActionInvocationParams {
    /// Subject the action operates on. `ActionDef.target` declares
    /// the expected `EntityKind`; the executor rejects on
    /// mismatch.
    pub subject: EntityRef,
    /// Caller-supplied parameter values, validated against
    /// `ActionDef.parameters`. Stored as a JSON value rather than
    /// a typed `HashMap<String, PropertyValue>` so this crate
    /// stays free of the `ox-core::types` enum surface — the
    /// executor walks the JSON during validation against the
    /// action's parameter schema.
    pub values: serde_json::Value,
    /// `Some` for replay-safe invocations; `None` for
    /// idempotency-best-effort. The executor reads
    /// `ActionDef.idempotency` to decide how to honour this.
    pub idempotency_key: Option<String>,
    /// `true` reroutes execution into a planner-only path that
    /// returns the effects without committing. Honoured
    /// regardless of `approval_policy`.
    pub dry_run: bool,
}

/// Typed counters of what the action affected. The agent's
/// `invoke_action` tool result envelope renders this so the LLM
/// has structured data for its next decision; the FE renders it
/// on the completion toast.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ActionAffected {
    pub nodes_created: u64,
    pub nodes_updated: u64,
    pub edges_created: u64,
    pub edges_updated: u64,
    /// Federation-backed actions write rows on the source
    /// adapter. Counted separately from `nodes_*` / `edges_*` so
    /// the operator can tell graph writes from upstream-source
    /// writes.
    pub rows_written: u64,
}

/// Caller identity. The executor passes this through to the
/// `prov:Activity` emit + the precondition rule evaluator (which
/// may branch on roles / scopes).
#[derive(Debug, Clone)]
pub struct Principal {
    /// Stable user identifier (UUID string).
    pub user_id: String,
    /// Workspace roles the user holds. Used by approval-policy
    /// precondition rules.
    pub roles: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn affected_default_is_zero() {
        let a = ActionAffected::default();
        assert_eq!(a.nodes_created, 0);
        assert_eq!(a.nodes_updated, 0);
        assert_eq!(a.edges_created, 0);
        assert_eq!(a.edges_updated, 0);
        assert_eq!(a.rows_written, 0);
    }

    #[test]
    fn pending_approval_carries_proposal_id() {
        let r = ActionResult::PendingApproval {
            proposal_id: "hp-12345".to_string(),
        };
        match r {
            ActionResult::PendingApproval { proposal_id } => assert_eq!(proposal_id, "hp-12345"),
            _ => panic!("expected PendingApproval variant"),
        }
    }

    #[test]
    fn dry_run_omits_provenance() {
        // The DryRun variant deliberately doesn't carry a
        // `provenance_id` — nothing committed, nothing to attribute.
        // The executor must NOT emit a `prov:Activity` row on the
        // dry-run path.
        let r = ActionResult::DryRun {
            affected: ActionAffected {
                nodes_created: 1,
                ..Default::default()
            },
        };
        match r {
            ActionResult::DryRun { affected } => assert_eq!(affected.nodes_created, 1),
            _ => panic!("expected DryRun variant"),
        }
    }

    #[test]
    fn principal_holds_roles() {
        let p = Principal {
            user_id: "u-7".to_string(),
            roles: vec!["admin".to_string(), "designer".to_string()],
        };
        assert_eq!(p.roles.len(), 2);
        assert!(p.roles.contains(&"admin".to_string()));
    }
}
