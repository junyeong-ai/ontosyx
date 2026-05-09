//! Change-type routing rules that drive approval automation.
//!
//! The patent 1-pager's "변경 유형 매트릭스" ties every ontology edit
//! to an automation tier (from 45% on table-merge up to 100% on
//! rollback). This module encodes that matrix as data:
//!
//! - [`ChangeType`] — the edit classes the matrix names, with one
//!   variant per routable row.
//! - [`ApprovalRouting`] — the destination: auto-approve, approve-
//!   with-notification, approve-required-unless-predicate, or
//!   always-approve.
//! - [`ChangeRoutingRule`] — a persistent `(workspace_id?, change_type)`
//!   → `ApprovalRouting` binding that the router consults when an
//!   edit request lands. Per-workspace overrides take precedence
//!   over the global default rows (via `priority`).
//!
//! The routing engine itself lives alongside the approval workflow,
//! so this module only ships the data shapes + defaults. A fresh
//! deploy seeds one rule per `ChangeType` with the patent-matrix
//! automation tier; workspaces override as needed.

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

ox_core::define_id_newtype!(ChangeRoutingRuleId);

/// Taxonomy of edit operations the routing matrix knows about.
/// The variant set is closed — adding a new kind is a deliberate
/// schema extension, not a silent addition. Every `OntologyEditOp`
/// classifies into exactly one of these.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ChangeType {
    /// Add a code to an existing CodeSystem. 80% auto per patent matrix.
    CodedValueCreate,
    /// Deprecate or retire a code (`deprecated_at` + `replaced_by`).
    /// 60% auto — historical queries need the rename chain preserved.
    CodedValueDeprecate,
    /// Update terminology registry content: glossary terms, value
    /// sets, concept maps, and adjacent vocabulary metadata. 70% auto.
    TerminologyRegistryUpdate,
    /// Bind or unbind a property to semantic registry targets
    /// (`Concept`, `ValueSet`, `CodeSystem`, `NotationPattern`, or
    /// `ValueRange`). 70% auto.
    SemanticBindingUpdate,
    /// Declare a new NotationPattern template. 60% auto.
    NotationPatternCreate,
    /// New CustomerSegment / analytical view. 55% auto.
    CustomerSegmentCreate,
    /// Rename a source column (schema evolution). 90% auto — the
    /// Temporal RenameCtx path handles backward-time queries, so
    /// the routing can trust most rename events.
    ColumnRename,
    /// Merge or split source tables. 45% auto — row-identity
    /// semantics can change, so default keeps a human in the loop.
    TableMerge,
    /// Register a new DataSource. 75% auto — connection string
    /// review + credential audit usually gets flagged.
    DataSourceRegister,
    /// Auto-detected stale concept suggestion. 95% auto — routing
    /// only emits a *proposal*; actual deletion is always HITL.
    StaleConceptDeprecate,
    /// Roll an ontology version back. 100% auto — rollback is
    /// mechanical and reversible; humans already decided "yes" by
    /// clicking the button.
    OntologyVersionRollback,
    /// Declare a new validation [`crate::rule::RuleDef`]. 65% auto —
    /// additive constraints can't produce new failures on historical
    /// data that already passed, so DataStewards + a passing
    /// validation run skip the queue; everyone else goes through
    /// review.
    RuleCreate,
    /// Modify an existing validation [`crate::rule::RuleDef`]. 55%
    /// auto — a tightened constraint may retroactively invalidate
    /// rows that previously passed, so only Admins + a passing
    /// validation run skip the queue.
    RuleModify,
    /// Remove a validation [`crate::rule::RuleDef`]. Always queues —
    /// deleting coverage is a governance decision, not a mechanical
    /// edit; no skip predicates.
    RuleDelete,
}

impl ChangeType {
    /// All variants in canonical order. Callers that need to iterate
    /// (e.g. routing-rule seed migration) use this instead of
    /// hand-maintained lists.
    pub const fn all() -> &'static [ChangeType] {
        &[
            Self::CodedValueCreate,
            Self::CodedValueDeprecate,
            Self::TerminologyRegistryUpdate,
            Self::SemanticBindingUpdate,
            Self::NotationPatternCreate,
            Self::CustomerSegmentCreate,
            Self::ColumnRename,
            Self::TableMerge,
            Self::DataSourceRegister,
            Self::StaleConceptDeprecate,
            Self::OntologyVersionRollback,
            Self::RuleCreate,
            Self::RuleModify,
            Self::RuleDelete,
        ]
    }

    /// Patent matrix default routing for this change type — used to
    /// seed the global rule set on fresh deploys. Workspace overrides
    /// still win via higher `priority`.
    pub fn default_routing(self) -> ApprovalRouting {
        match self {
            // 100% — mechanical / reversible.
            Self::OntologyVersionRollback => ApprovalRouting::AutoApprove,
            // 95% — auto only emits a *proposal*, so the "deprecate"
            // write is routine; the human still reviews the proposal
            // list.
            Self::StaleConceptDeprecate => ApprovalRouting::AutoApproveWithNotification {
                notify_roles: vec![RoleRef::DataSteward],
            },
            // 90% — Temporal RenameCtx carries backward-time queries
            // past the rename, so admins sail through. Non-admins
            // queue; the commit-path validate still enforces IR
            // integrity, so an Admin can't sneak a broken rename by.
            Self::ColumnRename => ApprovalRouting::ApprovalRequiredUnless {
                skip_predicates: vec![ApprovalSkipPredicate::AuthorHasRole {
                    role: RoleRef::Admin,
                }],
            },
            // 80% — new codes are low risk; the unique-key CHECK
            // catches duplicates so the routing can lean permissive.
            // Small batches (<5 codes) skip regardless of role; a
            // DataSteward with a larger batch still skips via the
            // role predicate.
            Self::CodedValueCreate => ApprovalRouting::ApprovalRequiredUnless {
                skip_predicates: vec![
                    ApprovalSkipPredicate::AuthorHasRole {
                        role: RoleRef::DataSteward,
                    },
                    ApprovalSkipPredicate::ChangeScopeBelow {
                        scope: ScopeKind::CodeCount,
                        threshold: 5,
                    },
                ],
            },
            // 75% — new source carries secret-ref handling, so the
            // routing is permissive for admins but keeps a review
            // loop for non-admin contributors.
            Self::DataSourceRegister => ApprovalRouting::ApprovalRequiredUnless {
                skip_predicates: vec![ApprovalSkipPredicate::AuthorHasRole {
                    role: RoleRef::Admin,
                }],
            },
            // 70% — terminology curation and semantic bindings are
            // routine steward work; validation still gates broken
            // references before commit.
            Self::SemanticBindingUpdate | Self::TerminologyRegistryUpdate => {
                ApprovalRouting::ApprovalRequiredUnless {
                    skip_predicates: vec![ApprovalSkipPredicate::AuthorHasRole {
                        role: RoleRef::DataSteward,
                    }],
                }
            }
            // 60% — notation templates + code deprecation both touch
            // identifier semantics; keep a reviewer by default.
            Self::NotationPatternCreate | Self::CodedValueDeprecate => {
                ApprovalRouting::ApprovalRequired
            }
            // 55% — segment definitions overlap with marketing /
            // analytics authority; human review is the norm.
            Self::CustomerSegmentCreate => ApprovalRouting::ApprovalRequired,
            // 45% — table-level merges can flip row identity
            // downstream; always human-reviewed by default.
            Self::TableMerge => ApprovalRouting::ApprovalRequired,
            // 65% — additive-constraint creations don't invalidate
            // rows that previously passed, so DataStewards sail
            // through; lower roles queue. The commit-path validate
            // still enforces IR integrity.
            Self::RuleCreate => ApprovalRouting::ApprovalRequiredUnless {
                skip_predicates: vec![ApprovalSkipPredicate::AuthorHasRole {
                    role: RoleRef::DataSteward,
                }],
            },
            // 55% — a tightened rule can retroactively invalidate
            // future rows that a passing validate-now cannot
            // anticipate, so only Admins skip; every other role
            // queues. The commit-path validate still enforces
            // current-state integrity.
            Self::RuleModify => ApprovalRouting::ApprovalRequiredUnless {
                skip_predicates: vec![ApprovalSkipPredicate::AuthorHasRole {
                    role: RoleRef::Admin,
                }],
            },
            // Always queues — removing coverage is a governance
            // decision, never mechanical.
            Self::RuleDelete => ApprovalRouting::ApprovalRequired,
        }
    }

    /// Default risk badge for the global seed row. Risk is audit/UI
    /// metadata; it does not affect routing decisions.
    pub fn default_risk_level(self) -> RiskLevel {
        match self {
            Self::CodedValueCreate
            | Self::TerminologyRegistryUpdate
            | Self::SemanticBindingUpdate
            | Self::StaleConceptDeprecate => RiskLevel::Low,
            Self::CodedValueDeprecate
            | Self::NotationPatternCreate
            | Self::CustomerSegmentCreate
            | Self::DataSourceRegister
            | Self::OntologyVersionRollback
            | Self::RuleCreate => RiskLevel::Medium,
            Self::ColumnRename | Self::TableMerge | Self::RuleModify | Self::RuleDelete => {
                RiskLevel::High
            }
        }
    }
}

/// Destination for a classified change. Variants are ordered from
/// most-permissive to least so a reader can eyeball routing posture
/// without a legend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ApprovalRouting {
    /// Apply immediately, no notification. For rollback + other
    /// actions the system considers mechanical.
    AutoApprove,
    /// Apply immediately, broadcast a notification to the named
    /// roles so stewards see the change in their feed.
    AutoApproveWithNotification { notify_roles: Vec<RoleRef> },
    /// Require approval, BUT skip the queue when any of the
    /// predicates evaluate to `true`. The most expressive branch —
    /// encodes "Admins sail through, high-scope edits get reviewed".
    ApprovalRequiredUnless {
        skip_predicates: Vec<ApprovalSkipPredicate>,
    },
    /// Always queue for approval.
    ApprovalRequired,
}

/// Dimension a [`ApprovalSkipPredicate::ChangeScopeBelow`]
/// predicate measures against. Each op that cares about size
/// declares its own scope vector via
/// [`crate::OntologyEditOp::scopes`]; the predicate picks the entry
/// matching `kind` and compares its value against `threshold`.
///
/// New scope dimensions (table count, entity count, ...) slot in
/// as additional variants without changing the predicate shape or
/// breaking existing matrix rows / workspace overrides.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ScopeKind {
    /// Number of coded values an op touches. Used by the
    /// CodedValue-lifecycle rows so small batches (<5 codes) can
    /// skip approval.
    CodeCount,
}

/// A single scope measurement an op declares at classification
/// time. `EditContext.scopes` is a `Vec` so a future op can
/// declare several scopes — e.g., a bulk CodedValue commit that
/// touches N codes across M code systems could emit both counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScopeValue {
    pub kind: ScopeKind,
    pub value: u32,
}

/// Predicates the `ApprovalRequiredUnless` branch evaluates. Every
/// predicate returns a boolean; the skip list is OR'd (first firing
/// predicate short-circuits to Apply).
///
/// The predicate set is intentionally minimal — only signals the
/// routing pipeline can evaluate WITHOUT executing the edit. That's
/// why `HasValidationPass` is absent: under the
/// `route → apply → validate → commit` pipeline, the validate step
/// runs AFTER routing, so at decision time no validation result
/// exists. Every matrix row that conceptually reads "role X with a
/// passing validation" collapses cleanly to "role X" — the commit
/// path still requires validate to succeed, so a bad IR never lands
/// even when the role gate waves it through.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ApprovalSkipPredicate {
    /// The author's role matches or outranks this reference. The
    /// named-field shape (rather than a tuple `AuthorHasRole(RoleRef)`)
    /// keeps the wire JSON readable and compatible with serde's
    /// `tag = "kind"` internal-tag discipline.
    AuthorHasRole { role: RoleRef },
    /// The op declares a scope of matching `scope` with a value
    /// strictly under `threshold`. Ops that don't declare the
    /// matching scope do NOT fire this predicate — "no scope"
    /// is treated distinctly from "zero-sized scope" so an op
    /// that never tracks its magnitude can't auto-skip by
    /// accident.
    ChangeScopeBelow { scope: ScopeKind, threshold: u32 },
}

/// Role reference. Declares the hierarchy at a symbolic level so the
/// routing rules don't hardcode workspace IDs.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum RoleRef {
    /// Full admin — top of the ladder.
    Admin,
    /// Data-steward / MD-team role — curates terminology + mappings.
    DataSteward,
    /// Analyst / read-heavy role.
    Analyst,
}

/// Persistent routing rule. `workspace_id == None` → global default
/// seeded at migration time; `Some(ws)` → per-workspace override.
/// Both rows can coexist; the router resolves by taking the higher
/// `priority` (workspace row wins via higher number at seed time).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct ChangeRoutingRule {
    pub id: ChangeRoutingRuleId,
    /// `None` for the global default row shipped by the migration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<Uuid>,
    pub change_type: ChangeType,
    pub routing: ApprovalRouting,
    pub risk_level: RiskLevel,
    pub priority: i32,
    pub created_at: DateTime<Utc>,
}

/// Risk tier — metadata for UI badging and audit filtering. Does
/// not influence routing directly (that's `ApprovalRouting`'s job).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
}

/// Evaluation result for a classified change request. Consumed by
/// the approval workflow to decide "apply vs queue".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditRoutingDecision {
    /// Apply immediately. `notify_roles` non-empty when the rule
    /// routes through `AutoApproveWithNotification`.
    Apply { notify_roles: Vec<RoleRef> },
    /// Queue for human approval.
    Queue,
}

/// Inputs the router needs to evaluate `ApprovalSkipPredicate`s.
/// Passing a bundle keeps future predicate additions additive —
/// new dimensions slot in as additional fields or scope variants
/// without forcing call-site rewrites.
#[derive(Debug, Clone, Default)]
pub struct EditContext {
    pub author_role: Option<RoleRef>,
    /// Scope metrics the op batch declared (aggregated with
    /// per-kind max across the batch). Typically 0 or 1 entries
    /// today; the `Vec` shape anticipates multi-dimensional ops.
    pub scopes: Vec<ScopeValue>,
}

/// Decide whether a classified change can apply automatically or
/// must queue for approval. Pure function — consumes the resolved
/// rule + context, returns the decision. Store lookup + override
/// precedence is the caller's responsibility (so tests don't need a
/// DB to pin the logic).
pub fn decide_edit_routing(routing: &ApprovalRouting, ctx: &EditContext) -> EditRoutingDecision {
    match routing {
        ApprovalRouting::AutoApprove => EditRoutingDecision::Apply {
            notify_roles: Vec::new(),
        },
        ApprovalRouting::AutoApproveWithNotification { notify_roles } => {
            EditRoutingDecision::Apply {
                notify_roles: notify_roles.clone(),
            }
        }
        ApprovalRouting::ApprovalRequiredUnless { skip_predicates } => {
            for pred in skip_predicates {
                if pred_passes(pred, ctx) {
                    return EditRoutingDecision::Apply {
                        notify_roles: Vec::new(),
                    };
                }
            }
            EditRoutingDecision::Queue
        }
        ApprovalRouting::ApprovalRequired => EditRoutingDecision::Queue,
    }
}

fn pred_passes(pred: &ApprovalSkipPredicate, ctx: &EditContext) -> bool {
    match pred {
        ApprovalSkipPredicate::AuthorHasRole { role } => {
            ctx.author_role.is_some_and(|r| role_outranks(r, *role))
        }
        ApprovalSkipPredicate::ChangeScopeBelow { scope, threshold } => {
            // "No matching scope declared" is NOT "zero-sized" —
            // an op that doesn't track `scope` must not auto-skip
            // by default. Require the op to explicitly declare a
            // value that's strictly under the threshold.
            ctx.scopes
                .iter()
                .find(|s| s.kind == *scope)
                .is_some_and(|s| s.value < *threshold)
        }
    }
}

/// Admin outranks DataSteward which outranks Analyst. Used for the
/// `AuthorHasRole` predicate so a rule that skips for DataSteward
/// also naturally skips for Admin.
fn role_outranks(actor: RoleRef, required: RoleRef) -> bool {
    fn rank(r: RoleRef) -> u8 {
        match r {
            RoleRef::Admin => 3,
            RoleRef::DataSteward => 2,
            RoleRef::Analyst => 1,
        }
    }
    rank(actor) >= rank(required)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn ctx_default() -> EditContext {
        EditContext::default()
    }

    #[test]
    fn auto_approve_applies_without_notification() {
        let decision = decide_edit_routing(&ApprovalRouting::AutoApprove, &ctx_default());
        assert_eq!(
            decision,
            EditRoutingDecision::Apply {
                notify_roles: vec![]
            }
        );
    }

    #[test]
    fn auto_approve_with_notification_surfaces_roles() {
        let routing = ApprovalRouting::AutoApproveWithNotification {
            notify_roles: vec![RoleRef::DataSteward],
        };
        assert_eq!(
            decide_edit_routing(&routing, &ctx_default()),
            EditRoutingDecision::Apply {
                notify_roles: vec![RoleRef::DataSteward]
            }
        );
    }

    #[test]
    fn approval_required_always_queues() {
        assert_eq!(
            decide_edit_routing(&ApprovalRouting::ApprovalRequired, &ctx_default()),
            EditRoutingDecision::Queue
        );
    }

    #[test]
    fn approval_required_unless_admin_role_skips() {
        let routing = ApprovalRouting::ApprovalRequiredUnless {
            skip_predicates: vec![ApprovalSkipPredicate::AuthorHasRole {
                role: RoleRef::DataSteward,
            }],
        };
        let ctx = EditContext {
            author_role: Some(RoleRef::Admin),
            ..Default::default()
        };
        // Admin outranks DataSteward — applies automatically.
        assert!(matches!(
            decide_edit_routing(&routing, &ctx),
            EditRoutingDecision::Apply { .. }
        ));
    }

    #[test]
    fn approval_required_unless_analyst_role_queues() {
        let routing = ApprovalRouting::ApprovalRequiredUnless {
            skip_predicates: vec![ApprovalSkipPredicate::AuthorHasRole {
                role: RoleRef::DataSteward,
            }],
        };
        let ctx = EditContext {
            author_role: Some(RoleRef::Analyst),
            ..Default::default()
        };
        // Analyst is below DataSteward — still queues.
        assert_eq!(
            decide_edit_routing(&routing, &ctx),
            EditRoutingDecision::Queue
        );
    }

    fn code_count_ctx(value: u32) -> EditContext {
        EditContext {
            scopes: vec![ScopeValue {
                kind: ScopeKind::CodeCount,
                value,
            }],
            ..Default::default()
        }
    }

    fn scope_below_routing(threshold: u32) -> ApprovalRouting {
        ApprovalRouting::ApprovalRequiredUnless {
            skip_predicates: vec![ApprovalSkipPredicate::ChangeScopeBelow {
                scope: ScopeKind::CodeCount,
                threshold,
            }],
        }
    }

    #[test]
    fn scope_below_threshold_skips_queue() {
        assert!(matches!(
            decide_edit_routing(&scope_below_routing(5), &code_count_ctx(3)),
            EditRoutingDecision::Apply { .. }
        ));
    }

    #[test]
    fn scope_at_threshold_does_not_skip() {
        // Strict `<` by design: 5 edits is the ceiling, not the floor.
        assert_eq!(
            decide_edit_routing(&scope_below_routing(5), &code_count_ctx(5)),
            EditRoutingDecision::Queue,
        );
    }

    #[test]
    fn scope_missing_kind_does_not_skip() {
        // "No matching scope declared" is NOT "zero-sized" — ops
        // that don't track the probed dimension must not auto-skip.
        let ctx = EditContext::default();
        assert_eq!(
            decide_edit_routing(&scope_below_routing(5), &ctx),
            EditRoutingDecision::Queue,
        );
    }

    #[test]
    fn all_change_types_have_distinct_variants() {
        // Guards against a future duplicate or missed variant on
        // `ChangeType::all()`. A growing matrix shouldn't silently
        // drop a kind from the canonical iteration list.
        let all = ChangeType::all();
        let mut seen: std::collections::HashSet<ChangeType> = std::collections::HashSet::new();
        for c in all {
            assert!(seen.insert(*c), "duplicate variant in all(): {c:?}");
        }
        // Patent matrix (11 rows) + three rule-lifecycle variants
        // added once OntologyEditOp grew CreateRule / UpdateRule /
        // DeleteRule — one row per lifecycle stage so the matrix can
        // treat rule deletion stricter than modification.
        assert_eq!(all.len(), 14);
    }

    #[test]
    fn every_change_type_has_a_default_routing() {
        for c in ChangeType::all() {
            // The match in default_routing is exhaustive by construction;
            // calling it on every variant proves no variant panics.
            let _ = c.default_routing();
            let _ = c.default_risk_level();
        }
    }

    #[test]
    fn rollback_defaults_to_plain_auto_approve() {
        assert_eq!(
            ChangeType::OntologyVersionRollback.default_routing(),
            ApprovalRouting::AutoApprove
        );
    }

    // ------------------------------------------------------------------
    // Rule-lifecycle routing: pins the role gates for the three
    // matrix rows authored by this crate. If a future matrix change
    // stops honouring role-based skipping, these tests catch it.
    // ------------------------------------------------------------------

    fn role_ctx(role: RoleRef) -> EditContext {
        EditContext {
            author_role: Some(role),
            scopes: Vec::new(),
        }
    }

    #[test]
    fn rule_create_data_steward_applies() {
        assert!(matches!(
            decide_edit_routing(
                &ChangeType::RuleCreate.default_routing(),
                &role_ctx(RoleRef::DataSteward),
            ),
            EditRoutingDecision::Apply { .. }
        ));
    }

    #[test]
    fn rule_create_analyst_queues() {
        assert_eq!(
            decide_edit_routing(
                &ChangeType::RuleCreate.default_routing(),
                &role_ctx(RoleRef::Analyst),
            ),
            EditRoutingDecision::Queue,
        );
    }

    #[test]
    fn rule_modify_gates_on_admin_role() {
        // DataSteward can author a rule (RuleCreate) but not
        // tighten an existing one — the matrix is stricter because
        // tightening can retroactively break rows the current
        // validate can't anticipate.
        assert_eq!(
            decide_edit_routing(
                &ChangeType::RuleModify.default_routing(),
                &role_ctx(RoleRef::DataSteward),
            ),
            EditRoutingDecision::Queue,
        );
        assert_eq!(
            decide_edit_routing(
                &ChangeType::RuleModify.default_routing(),
                &role_ctx(RoleRef::Analyst),
            ),
            EditRoutingDecision::Queue,
        );
        assert!(matches!(
            decide_edit_routing(
                &ChangeType::RuleModify.default_routing(),
                &role_ctx(RoleRef::Admin),
            ),
            EditRoutingDecision::Apply { .. }
        ));
    }

    #[test]
    fn rule_delete_always_queues() {
        // Removing coverage is a governance decision — the matrix
        // has no skip predicates for RuleDelete by design.
        for role in [RoleRef::Admin, RoleRef::DataSteward, RoleRef::Analyst] {
            assert_eq!(
                decide_edit_routing(&ChangeType::RuleDelete.default_routing(), &role_ctx(role),),
                EditRoutingDecision::Queue,
                "RuleDelete must queue for role={role:?}",
            );
        }
    }

    #[test]
    fn table_merge_defaults_to_approval_required() {
        assert_eq!(
            ChangeType::TableMerge.default_routing(),
            ApprovalRouting::ApprovalRequired
        );
    }

    #[test]
    fn routing_round_trips_through_json_with_internal_tag() {
        let r = ApprovalRouting::AutoApproveWithNotification {
            notify_roles: vec![RoleRef::DataSteward],
        };
        let j = serde_json::to_value(&r).unwrap();
        assert_eq!(
            j,
            serde_json::json!({
                "kind": "auto_approve_with_notification",
                "notify_roles": ["data_steward"]
            })
        );
        let back: ApprovalRouting = serde_json::from_value(j).unwrap();
        assert_eq!(back, r);
    }

    // ------------------------------------------------------------------
    // Wire-contract parity: every `default_routing()` Rust value must
    // round-trip through JSON without loss. This catches silent drift
    // between the in-code matrix and the seed INSERT in
    // `migrations/0001_schema.sql` — if either side grows a field or
    // renames a variant, the DB rows would deserialise into a shape
    // `ApprovalRouting` can't represent (or vice-versa).
    //
    // The test runs on every `ChangeType::all()` variant — adding a
    // matrix row means this test covers it automatically.
    // ------------------------------------------------------------------

    #[test]
    fn every_default_routing_round_trips_through_json() {
        for c in ChangeType::all() {
            let original = c.default_routing();
            let j =
                serde_json::to_value(&original).unwrap_or_else(|e| panic!("serialize {c:?}: {e}"));
            let back: ApprovalRouting = serde_json::from_value(j.clone())
                .unwrap_or_else(|e| panic!("deserialize {c:?} from {j}: {e}"));
            assert_eq!(back, original, "round-trip drift on {c:?}");
        }
    }

    #[test]
    fn scope_value_new_shape_round_trips() {
        // Guards the JSON shape the seed row for CodedValueCreate
        // relies on: `{"kind":"change_scope_below","scope":"code_count","threshold":5}`.
        // Any future rename of `scope`/`threshold`/`code_count` breaks
        // this test AND the migration seed — surfacing the drift at
        // CI instead of at `cargo run`.
        let pred = ApprovalSkipPredicate::ChangeScopeBelow {
            scope: ScopeKind::CodeCount,
            threshold: 5,
        };
        let j = serde_json::to_value(&pred).unwrap();
        assert_eq!(
            j,
            serde_json::json!({
                "kind": "change_scope_below",
                "scope": "code_count",
                "threshold": 5
            })
        );
        let back: ApprovalSkipPredicate = serde_json::from_value(j).unwrap();
        assert_eq!(back, pred);
    }

    #[test]
    fn scope_kind_serialises_as_snake_case_string() {
        // The enum has the `#[serde(rename_all = "snake_case")]`
        // attribute and no internal tag — each variant is a bare
        // string. Pinning that shape explicitly so a future
        // reviewer doesn't drift into a tagged-union form.
        let j = serde_json::to_value(ScopeKind::CodeCount).unwrap();
        assert_eq!(j, serde_json::Value::String("code_count".into()));
        let back: ScopeKind = serde_json::from_value(j).unwrap();
        assert_eq!(back, ScopeKind::CodeCount);
    }
}
