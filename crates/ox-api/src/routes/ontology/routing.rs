//! Shared routing helpers for the ontology edit surface.
//!
//! Both the creation endpoint (`POST /api/ontologies`) and the
//! follow-up edit endpoint (`POST /api/ontologies/{id}/edits`) run
//! each operation through the Phase-6 approval-routing matrix. The
//! matrix decides per-op whether the change applies immediately or
//! queues for review.
//!
//! ## Pipeline ordering
//!
//! Routing is **pure**: it reads only the op's static classification
//! (`classify_change_type`, `code_count_delta`) and the caller's
//! role. It carries no `validation_passed` input — the matrix
//! intentionally does not encode "skip if validation passes" because
//! every skip predicate like that would be trivially satisfied under
//! the commit pipeline (validate always runs before persistence).
//!
//! The handlers therefore run routing **first**:
//!
//! 1. [`verify_ops_apply`] — pure, fast. Fails 409 on a queue decision.
//! 2. Apply ops to an `OntologyIR` clone (fails 422 on op error).
//! 3. `ir.validate()` (fails 422 on cross-ref integrity error).
//! 4. Commit (single storage transaction).
//!
//! Any rule that needs to block unsafe writes leans on step 3 —
//! the commit path refuses an invalid IR regardless of how
//! permissive routing was.

use std::collections::HashMap;

use ox_ontology::OntologyEditOp;
use ox_ontology::change_routing::{
    EditContext, EditRoutingDecision, RoleRef, ScopeKind, ScopeValue, decide_edit_routing,
};

use crate::error::AppError;
use crate::principal::{PlatformRole, Principal};
use crate::state::AppState;

/// Collapse a batch's per-op scope declarations into the single
/// per-kind max that `ChangeScopeBelow` compares against. Max (not
/// sum) because the matrix asks "is this batch's worst-case scope
/// still small?" — if any op pushes past the threshold, the batch
/// should queue.
fn aggregate_scopes(ops: &[OntologyEditOp]) -> Vec<ScopeValue> {
    let mut by_kind: HashMap<ScopeKind, u32> = HashMap::new();
    for op in ops {
        for sv in op.scopes() {
            by_kind
                .entry(sv.kind)
                .and_modify(|v| *v = (*v).max(sv.value))
                .or_insert(sv.value);
        }
    }
    by_kind
        .into_iter()
        .map(|(kind, value)| ScopeValue { kind, value })
        .collect()
}

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
/// or route through the approval surface. Missing routing rows
/// surface as 500 so ops can notice the seed migration wasn't
/// applied.
pub(super) async fn verify_ops_apply(
    state: &AppState,
    principal: &Principal,
    ops: &[OntologyEditOp],
) -> Result<(), AppError> {
    let ctx = EditContext {
        author_role: Some(role_ref_of(principal)),
        scopes: aggregate_scopes(ops),
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
