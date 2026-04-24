//! Shared routing helpers for the ontology edit surface.
//!
//! Both the creation endpoint (`POST /api/ontologies`) and the
//! follow-up edit endpoint (`POST /api/ontologies/{id}/edits`) run
//! each operation through the Phase-6 approval-routing matrix. The
//! matrix decides per-op whether the change applies immediately or
//! queues for review.
//!
//! ## Two-phase contract
//!
//! Routing evaluates **after** [`ox_ontology::OntologyIR::validate`]
//! succeeds — that's why this module only exposes the verification
//! helper and expects the caller to pass the validate outcome. The
//! `HasValidationPass` skip predicate in the routing matrix (see
//! [`ox_ontology::change_routing::ApprovalSkipPredicate`]) is
//! meaningful exactly because routing sees the real validate result.
//!
//! The order each handler follows:
//!
//! 1. Apply every op to an IR clone (fails → 422).
//! 2. `ir.validate()` (fails → 422).
//! 3. [`verify_ops_apply`] with `validation_passed = true`.
//! 4. Commit (or fail → 409 when any op queues).
//!
//! Swapping the order ("route before validate") would make
//! `HasValidationPass` dead code, which is why the old single-phase
//! shape was replaced.

use ox_ontology::OntologyEditOp;
use ox_ontology::change_routing::{
    EditContext, EditRoutingDecision, RoleRef, decide_edit_routing,
};

use crate::error::AppError;
use crate::principal::{PlatformRole, Principal};
use crate::state::AppState;

/// Map the platform role to the routing matrix's `RoleRef`.
///
/// The matrix operates on logical roles (admin / data-steward /
/// analyst) rather than platform roles so workspace policy can
/// reason about them symbolically. Designers map to DataSteward —
/// the "curates terminology + mappings" tier — and Viewers fall
/// through to Analyst so no skip predicate ever fires.
pub(super) fn role_ref_of(principal: &Principal) -> RoleRef {
    match principal.role {
        PlatformRole::Admin => RoleRef::Admin,
        PlatformRole::Designer => RoleRef::DataSteward,
        PlatformRole::Viewer => RoleRef::Analyst,
    }
}

/// Verify every op in the batch resolves to `Apply` under the
/// current routing matrix. Pure — reads routing rules and returns a
/// decision; never mutates state.
///
/// Returns `Ok(())` only when every op resolves to `Apply`. On any
/// `Queue` decision, returns a 409 so the caller can split the batch
/// or route through the approval surface.
///
/// `validation_passed` feeds the `HasValidationPass` skip predicate —
/// callers must run `ir.validate()` first and pass its real outcome
/// (typically `true`, because an error in validate should short-
/// circuit the handler with 422 before reaching this helper).
/// Missing routing rows surface as 500 so ops can notice the seed
/// migration wasn't applied.
pub(super) async fn verify_ops_apply(
    state: &AppState,
    principal: &Principal,
    ops: &[OntologyEditOp],
    validation_passed: bool,
) -> Result<(), AppError> {
    let ctx = EditContext {
        author_role: Some(role_ref_of(principal)),
        validation_passed,
        code_count_delta: ops
            .iter()
            .map(|op| op.code_count_delta())
            .max()
            .unwrap_or(0),
    };

    for op in ops {
        let rule = state
            .store
            .resolve_change_routing(op.classify_change_type())
            .await
            .map_err(AppError::from)?;
        let Some(rule) = rule else {
            return Err(AppError::internal(
                "missing change_routing_rules row — check the schema seed",
            ));
        };
        if matches!(
            decide_edit_routing(&rule.routing, &ctx),
            EditRoutingDecision::Queue
        ) {
            return Err(AppError::conflict(
                "edit queued for approval — automation policy requires review",
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::principal::PlatformRole;

    fn principal_with_role(role: PlatformRole) -> Principal {
        Principal {
            id: "00000000-0000-0000-0000-000000000000".into(),
            email: "test@example.com".into(),
            role,
        }
    }

    #[test]
    fn platform_admin_maps_to_matrix_admin() {
        assert_eq!(
            role_ref_of(&principal_with_role(PlatformRole::Admin)),
            RoleRef::Admin,
        );
    }

    #[test]
    fn platform_designer_maps_to_data_steward() {
        // Designers sit on the Phase-6 DataSteward tier so the
        // "curates terminology + mappings" skip predicates fire for
        // them on routine edits (glossary term create etc.).
        assert_eq!(
            role_ref_of(&principal_with_role(PlatformRole::Designer)),
            RoleRef::DataSteward,
        );
    }

    #[test]
    fn platform_viewer_maps_to_analyst() {
        // Analyst is the least-privileged tier; no skip predicate
        // that keys on `AuthorHasRole` fires for it.
        assert_eq!(
            role_ref_of(&principal_with_role(PlatformRole::Viewer)),
            RoleRef::Analyst,
        );
    }
}
