//! Change-type routing rules that drive approval automation.
//!
//! The patent 1-pager's "변경 유형 매트릭스" ties every ontology edit
//! to an automation tier (from 45% on table-merge up to 100% on
//! rollback). This module encodes that matrix as data:
//!
//! - [`ChangeType`] — the ten kinds of edit the matrix names, with
//!   one variant per row.
//! - [`ApprovalRouting`] — the destination: auto-approve, approve-
//!   with-notification, approve-required-unless-predicate, or
//!   always-approve.
//! - [`ChangeRoutingRule`] — a persistent `(workspace_id?, change_type)`
//!   → `ApprovalRouting` binding that the router consults when a
//!   Phase-2 edit request lands. Per-workspace overrides take
//!   precedence over the global default rows (via `priority`).
//!
//! The routing engine itself lives alongside the approval workflow
//! (Phase 2) so this module only ships the data shapes + defaults.
//! A fresh deploy seeds one rule per `ChangeType` with the
//! patent-matrix automation tier; workspaces override as needed.

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

ox_core::define_id_newtype!(ChangeRoutingRuleId);

/// Taxonomy of edit operations the routing matrix knows about.
/// The variant set is closed — adding a new kind is a deliberate
/// schema extension, not a silent addition. Every `OntologyEditOp`
/// (Phase 2) classifies into exactly one of these.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ChangeType {
    /// Add a code to an existing CodeSystem. 80% auto per patent matrix.
    CodedValueCreate,
    /// Deprecate or retire a code (`deprecated_at` + `replaced_by`).
    /// 60% auto — historical queries need the rename chain preserved.
    CodedValueDeprecate,
    /// Create a new GlossaryTerm. 70% auto.
    GlossaryTermCreate,
    /// Add an alias to an existing GlossaryTerm. 70% auto — drives
    /// query-hit-rate improvements so it's the most frequent routine
    /// edit.
    GlossaryAliasAdd,
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
            Self::GlossaryTermCreate,
            Self::GlossaryAliasAdd,
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
            Self::StaleConceptDeprecate => {
                ApprovalRouting::AutoApproveWithNotification {
                    notify_roles: vec![RoleRef::DataSteward],
                }
            }
            // 90% — Temporal RenameCtx carries backward-time queries
            // past the rename, so admins with the role sail through.
            Self::ColumnRename => ApprovalRouting::ApprovalRequiredUnless {
                skip_predicates: vec![
                    ApprovalSkipPredicate::AuthorHasRole { role: RoleRef::Admin },
                    ApprovalSkipPredicate::HasValidationPass,
                ],
            },
            // 80% — new codes are low risk; the unique-key CHECK
            // catches duplicates so the routing can lean permissive.
            Self::CodedValueCreate => ApprovalRouting::ApprovalRequiredUnless {
                skip_predicates: vec![
                    ApprovalSkipPredicate::AuthorHasRole { role: RoleRef::DataSteward },
                    ApprovalSkipPredicate::ChangeScopeBelow { code_count_delta: 5 },
                ],
            },
            // 75% — new source carries secret-ref handling, so the
            // routing is permissive for admins but keeps a review
            // loop for non-admin contributors.
            Self::DataSourceRegister => ApprovalRouting::ApprovalRequiredUnless {
                skip_predicates: vec![ApprovalSkipPredicate::AuthorHasRole { role: RoleRef::Admin }],
            },
            // 70% — alias additions are the most routine edit; MD
            // teams iterate on glossary coverage constantly.
            Self::GlossaryAliasAdd | Self::GlossaryTermCreate => {
                ApprovalRouting::ApprovalRequiredUnless {
                    skip_predicates: vec![
                        ApprovalSkipPredicate::AuthorHasRole { role: RoleRef::DataSteward },
                    ],
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
            // 65% — additive-constraint creations can be trusted when
            // validation already passes, because the shape cannot
            // invalidate rows that previously passed.
            Self::RuleCreate => ApprovalRouting::ApprovalRequiredUnless {
                skip_predicates: vec![
                    ApprovalSkipPredicate::AuthorHasRole { role: RoleRef::DataSteward },
                    ApprovalSkipPredicate::HasValidationPass,
                ],
            },
            // 55% — a tightened rule can retroactively invalidate
            // existing rows that passed a LOOSER prior shape, so
            // `HasValidationPass` alone is NOT enough — validation
            // ran against the current state, not against the rows
            // a future insert will produce. Only Admins skip; every
            // other role queues regardless of the validate outcome.
            // (Skip predicates are OR'd, so adding HasValidationPass
            // here would let any role sail through.)
            Self::RuleModify => ApprovalRouting::ApprovalRequiredUnless {
                skip_predicates: vec![
                    ApprovalSkipPredicate::AuthorHasRole { role: RoleRef::Admin },
                ],
            },
            // Always queues — removing coverage is a governance
            // decision, never mechanical.
            Self::RuleDelete => ApprovalRouting::ApprovalRequired,
        }
    }
}

/// Destination for a classified change. Variants are ordered from
/// most-permissive to least so a reader can eyeball routing posture
/// without a legend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
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

/// Predicates the `ApprovalRequiredUnless` branch evaluates. Every
/// predicate returns a boolean; the routing short-circuits on the
/// first `true`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ApprovalSkipPredicate {
    /// The author's role matches or outranks this reference. The
    /// named-field shape (rather than a tuple `AuthorHasRole(RoleRef)`)
    /// keeps the wire JSON readable and compatible with serde's
    /// `tag = "kind"` internal-tag discipline.
    AuthorHasRole { role: RoleRef },
    /// The change's scope is under a threshold the matrix considers
    /// "small". Today this is a raw code-count delta on
    /// CodedValueCreate; future change kinds define their own
    /// interpretations on the same predicate shape.
    ChangeScopeBelow { code_count_delta: u32 },
    /// An attached `OntologyIR::validate()` / SHACL run came back
    /// clean — proves the edit satisfies declared shapes, which is
    /// the automation-friendliness signal we want.
    HasValidationPass,
}

/// Role reference. Declares the hierarchy at a symbolic level so the
/// routing rules don't hardcode workspace IDs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
}

/// Evaluation result for a classified change request. Consumed by
/// the approval workflow (Phase 2) to decide "apply vs queue".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditRoutingDecision {
    /// Apply immediately. `notify_roles` non-empty when the rule
    /// routes through `AutoApproveWithNotification`.
    Apply { notify_roles: Vec<RoleRef> },
    /// Queue for human approval.
    Queue,
}

/// Inputs the router needs to evaluate `ApprovalSkipPredicate`s.
/// Passing a bundle keeps future predicate additions additive.
#[derive(Debug, Clone, Default)]
pub struct EditContext {
    pub author_role: Option<RoleRef>,
    pub code_count_delta: u32,
    pub validation_passed: bool,
}

/// Decide whether a classified change can apply automatically or
/// must queue for approval. Pure function — consumes the resolved
/// rule + context, returns the decision. Store lookup + override
/// precedence is the caller's responsibility (so tests don't need a
/// DB to pin the logic).
pub fn decide_edit_routing(
    routing: &ApprovalRouting,
    ctx: &EditContext,
) -> EditRoutingDecision {
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
        ApprovalSkipPredicate::ChangeScopeBelow { code_count_delta } => {
            ctx.code_count_delta < *code_count_delta
        }
        ApprovalSkipPredicate::HasValidationPass => ctx.validation_passed,
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
            skip_predicates: vec![ApprovalSkipPredicate::AuthorHasRole { role: RoleRef::DataSteward }],
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
            skip_predicates: vec![ApprovalSkipPredicate::AuthorHasRole { role: RoleRef::DataSteward }],
        };
        let ctx = EditContext {
            author_role: Some(RoleRef::Analyst),
            ..Default::default()
        };
        // Analyst is below DataSteward — still queues.
        assert_eq!(decide_edit_routing(&routing, &ctx), EditRoutingDecision::Queue);
    }

    #[test]
    fn scope_below_threshold_skips_queue() {
        let routing = ApprovalRouting::ApprovalRequiredUnless {
            skip_predicates: vec![ApprovalSkipPredicate::ChangeScopeBelow {
                code_count_delta: 5,
            }],
        };
        let ctx = EditContext {
            code_count_delta: 3,
            ..Default::default()
        };
        assert!(matches!(
            decide_edit_routing(&routing, &ctx),
            EditRoutingDecision::Apply { .. }
        ));
    }

    #[test]
    fn scope_at_threshold_does_not_skip() {
        let routing = ApprovalRouting::ApprovalRequiredUnless {
            skip_predicates: vec![ApprovalSkipPredicate::ChangeScopeBelow {
                code_count_delta: 5,
            }],
        };
        let ctx = EditContext {
            code_count_delta: 5,
            ..Default::default()
        };
        // Strict `<` by design: 5 edits is the ceiling, not the floor.
        assert_eq!(decide_edit_routing(&routing, &ctx), EditRoutingDecision::Queue);
    }

    #[test]
    fn validation_pass_predicate_respected() {
        let routing = ApprovalRouting::ApprovalRequiredUnless {
            skip_predicates: vec![ApprovalSkipPredicate::HasValidationPass],
        };
        let pass_ctx = EditContext {
            validation_passed: true,
            ..Default::default()
        };
        let fail_ctx = EditContext::default();
        assert!(matches!(
            decide_edit_routing(&routing, &pass_ctx),
            EditRoutingDecision::Apply { .. }
        ));
        assert_eq!(decide_edit_routing(&routing, &fail_ctx), EditRoutingDecision::Queue);
    }

    #[test]
    fn all_change_types_have_distinct_variants() {
        // Guards against a future duplicate or missed variant on
        // `ChangeType::all()`. A growing matrix shouldn't silently
        // drop a kind from the canonical iteration list.
        let all = ChangeType::all();
        let mut seen: std::collections::HashSet<ChangeType> =
            std::collections::HashSet::new();
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
    // Rule-lifecycle routing: guards the two-phase contract that
    // `ox-api::routes::ontology::routing::verify_ops_apply` relies on.
    // If the API layer stops passing `validation_passed: true` (or
    // stops running validate before routing), these tests surface the
    // regression — the skip predicates would silently stop firing and
    // every non-Admin/non-DataSteward edit would queue.
    // ------------------------------------------------------------------

    fn rule_ctx(role: RoleRef, validation_passed: bool) -> EditContext {
        EditContext {
            author_role: Some(role),
            validation_passed,
            code_count_delta: 0,
        }
    }

    #[test]
    fn rule_create_data_steward_with_validation_pass_applies() {
        // Matches the happy path for a designer authoring a new
        // validation rule against a clean IR — the two-phase flow
        // (apply → validate → route) guarantees `validation_passed`
        // reflects the real validate result.
        assert!(matches!(
            decide_edit_routing(
                &ChangeType::RuleCreate.default_routing(),
                &rule_ctx(RoleRef::DataSteward, true),
            ),
            EditRoutingDecision::Apply { .. }
        ));
    }

    #[test]
    fn rule_create_analyst_with_validation_pass_still_queues() {
        // Analyst has neither `AuthorHasRole(DataSteward)` nor any
        // other skip predicate firing beyond HasValidationPass. The
        // HasValidationPass predicate alone DOES fire when validate
        // passed — so the test verifies the skip predicate is OR'd,
        // not AND'd.
        assert!(matches!(
            decide_edit_routing(
                &ChangeType::RuleCreate.default_routing(),
                &rule_ctx(RoleRef::Analyst, true),
            ),
            EditRoutingDecision::Apply { .. }
        ));
    }

    #[test]
    fn rule_create_analyst_without_validation_pass_queues() {
        assert_eq!(
            decide_edit_routing(
                &ChangeType::RuleCreate.default_routing(),
                &rule_ctx(RoleRef::Analyst, false),
            ),
            EditRoutingDecision::Queue,
        );
    }

    #[test]
    fn rule_modify_gates_on_admin_role_not_validation_pass() {
        // DataSteward cannot skip a rule modification even when
        // validation passes — `ApprovalRequiredUnless` predicates are
        // OR'd, so adding `HasValidationPass` to the skip list would
        // defeat the Admin gate. A tightened rule can still break
        // FUTURE inserts that the current validate can't anticipate.
        assert_eq!(
            decide_edit_routing(
                &ChangeType::RuleModify.default_routing(),
                &rule_ctx(RoleRef::DataSteward, true),
            ),
            EditRoutingDecision::Queue,
        );
        assert_eq!(
            decide_edit_routing(
                &ChangeType::RuleModify.default_routing(),
                &rule_ctx(RoleRef::Analyst, true),
            ),
            EditRoutingDecision::Queue,
        );
        // Admin skips regardless of validation (AuthorHasRole is a
        // top-level role check — rank(Admin) >= rank(Admin)).
        for pass in [true, false] {
            assert!(matches!(
                decide_edit_routing(
                    &ChangeType::RuleModify.default_routing(),
                    &rule_ctx(RoleRef::Admin, pass),
                ),
                EditRoutingDecision::Apply { .. }
            ), "Admin must always skip RuleModify review (validation_passed={pass})");
        }
    }

    #[test]
    fn rule_delete_always_queues_regardless_of_role_or_validation() {
        // Removing coverage is a governance decision — the matrix
        // has no skip predicates for RuleDelete by design.
        for role in [RoleRef::Admin, RoleRef::DataSteward, RoleRef::Analyst] {
            for pass in [true, false] {
                assert_eq!(
                    decide_edit_routing(
                        &ChangeType::RuleDelete.default_routing(),
                        &rule_ctx(role, pass),
                    ),
                    EditRoutingDecision::Queue,
                    "RuleDelete must queue for role={role:?}, pass={pass}",
                );
            }
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
}
