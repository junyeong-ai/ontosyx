//! `ActionDef` — first-class write operations over the ontology.
//!
//! Where `FunctionDef` is a pure derivation, `ActionDef` is the
//! ontology-level contract for *mutation*. Every state-changing
//! operation that a user or agent can invoke is declared here so
//! that:
//!
//! - the platform knows which fields it may write,
//! - pre- and post-conditions reference `RuleId`s that get checked
//!   against the physical source under one transaction,
//! - idempotency is declared at the contract level (a duplicate
//!   `charge_customer` call with the same idempotency key returns
//!   the previous result instead of double-charging), and
//! - approval policy is explicit — no mutation escapes the agent
//!   tool loop without a preview unless the action is marked
//!   `Automatic`.
//!
//! Actions are single-source by contract. Cross-source workflows
//! are Saga-shaped and outside the scope of one action.

use chrono::Duration;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use ox_core::i18n::LocalizedText;

use crate::ir::{EdgeTypeId, NodeTypeId};

ox_core::define_id_newtype!(
    /// Stable identifier for an `ActionDef`.
    ActionId
);

ox_core::define_id_newtype!(
    /// Stable identifier for a `RuleDef`. Declared here instead of
    /// `rule.rs` to break the import cycle — `ActionDef` references
    /// rules for preconditions / postconditions.
    RuleId
);

/// Named, approval-aware mutation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
pub struct ActionDef {
    pub id: ActionId,

    pub name: String,

    #[serde(default)]
    pub description: LocalizedText,

    /// Ontology target of the action. `ActionTarget::NodeType`
    /// covers "create / update / delete a User", `ActionTarget::EdgeType`
    /// covers "connect two existing objects".
    pub target: ActionTarget,

    pub kind: ActionKind,

    /// Rules evaluated *before* the action runs — failure aborts.
    /// Typical uses: authorization, referential-integrity checks,
    /// quota guards.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub preconditions: Vec<RuleId>,

    /// Rules evaluated *after* the action commits, inside the same
    /// transaction — failure rolls back. Used for invariants that
    /// depend on the post-state (e.g. account balance ≥ 0).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub postconditions: Vec<RuleId>,

    /// Idempotency contract. See `IdempotencyPolicy`.
    #[serde(default)]
    pub idempotency: IdempotencyPolicy,

    /// Approval contract. See `ApprovalPolicy`.
    #[serde(default)]
    pub approval: ApprovalPolicy,
}

/// What the action writes to.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ActionTarget {
    NodeType { node_type_id: NodeTypeId },
    EdgeType { edge_type_id: EdgeTypeId },
}

/// Mutation shape.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ActionKind {
    Create,
    Update,
    Upsert,
    Delete,
    /// Catch-all for domain-specific mutations (state transitions,
    /// counter increments) that do not map cleanly onto CRUD. The
    /// semantics are carried by `preconditions` + `postconditions`
    /// rather than the kind tag.
    Custom,
}

/// Idempotency contract.
///
/// Actions with a non-`None` policy accept an opaque key from the
/// caller; a replay within the window returns the cached outcome
/// instead of re-executing. The platform persists the key +
/// fingerprint + result; beyond the window the key expires and a
/// repeat call runs fresh.
#[derive(
    Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum IdempotencyPolicy {
    /// No idempotency. Every call runs independently.
    #[default]
    None,
    /// The caller supplies an opaque key; results are cached for
    /// `window_seconds`. A repeat within the window returns the
    /// cached outcome.
    Keyed {
        /// Minimum supported window: 0 (cache forever). 0 is
        /// permitted because a Stripe-style `idempotency_key` never
        /// ages out in normal operation.
        window_seconds: u64,
    },
}

impl IdempotencyPolicy {
    /// Convenience: build a `Keyed` policy from a `chrono::Duration`.
    /// Negative durations clamp to 0 (no window — key never expires).
    pub fn keyed(window: Duration) -> Self {
        IdempotencyPolicy::Keyed {
            window_seconds: window.num_seconds().max(0) as u64,
        }
    }
}

/// Approval contract.
///
/// `Automatic` actions are executed without a prompt; `RequireApproval`
/// pauses the agent loop until a human confirms. Approvers are
/// declared by workspace role. A dangerous action (e.g. mass
/// `Delete`) should always require approval — the platform enforces
/// this by refusing to compile a tool manifest whose approval is
/// `Automatic` on a `Delete` target.
#[derive(
    Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ApprovalPolicy {
    #[default]
    Automatic,
    RequireApproval {
        /// Workspace roles that may approve. At least one of these
        /// must confirm before the action executes.
        approver_roles: Vec<String>,
        /// Human-readable rationale shown in the approval UI.
        #[serde(default)]
        rationale: LocalizedText,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_idempotency_is_none() {
        assert!(matches!(
            IdempotencyPolicy::default(),
            IdempotencyPolicy::None
        ));
    }

    #[test]
    fn keyed_helper_clamps_negative_durations_to_zero() {
        let p = IdempotencyPolicy::keyed(Duration::seconds(-30));
        assert!(matches!(p, IdempotencyPolicy::Keyed { window_seconds: 0 }));
    }

    #[test]
    fn action_roundtrips_through_json() {
        let a = ActionDef {
            id: ActionId::new("act-create-user"),
            name: "create_user".into(),
            description: LocalizedText::default(),
            target: ActionTarget::NodeType {
                node_type_id: NodeTypeId::new("nt-user"),
            },
            kind: ActionKind::Create,
            preconditions: vec![RuleId::new("rule-email-required")],
            postconditions: vec![],
            idempotency: IdempotencyPolicy::keyed(Duration::hours(24)),
            approval: ApprovalPolicy::Automatic,
        };
        let j = serde_json::to_value(&a).unwrap();
        let back: ActionDef = serde_json::from_value(j).unwrap();
        assert_eq!(back, a);
    }
}
