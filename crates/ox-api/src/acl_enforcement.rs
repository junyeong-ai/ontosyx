//! ACL enforcement bridge — load policies from the store, scope
//! them into the runtime task-local the Cypher pipeline reads, and
//! apply them post-hoc on the federation path (which executes a
//! DataFusion `LogicalPlan` and therefore never reaches the Cypher
//! rewriter).

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use ox_query_ir::query::QueryResult;
use ox_core::types::PropertyValue;
use ox_runtime::cypher::{
    AclAction, AclPolicySpec, AclSnapshot, RequestPrincipal,
};
use ox_store::{AclPolicy, AclStore};

use crate::error::AppError;
use crate::principal::Principal;
use crate::workspace::WorkspaceContext;

/// Look up the effective ACL policies for `principal` in the
/// current workspace and project them onto the rewriter-internal
/// [`AclSnapshot`] shape. The snapshot is sorted priority-desc by
/// the underlying store query.
pub async fn load_acl_snapshot(
    store: &dyn AclStore,
    principal: &Principal,
    ws: &WorkspaceContext,
) -> Result<Arc<AclSnapshot>, AppError> {
    let user_id = principal.user_uuid().ok();
    let policies = store
        .get_effective_policies(
            principal.role.as_str(),
            ws.workspace_role.as_str(),
            user_id,
        )
        .await
        .map_err(AppError::from)?;
    Ok(Arc::new(snapshot_from_policies(&policies)))
}

/// Build a [`RequestPrincipal`] from the HTTP-layer
/// [`Principal`] + [`WorkspaceContext`] for runtime task-local
/// scoping.
pub fn request_principal(principal: &Principal, ws: &WorkspaceContext) -> Option<RequestPrincipal> {
    let id = principal.user_uuid().ok()?;
    Some(RequestPrincipal::new(id, ws.workspace_role.as_str()))
}

fn snapshot_from_policies(policies: &[AclPolicy]) -> AclSnapshot {
    let mut specs = Vec::with_capacity(policies.len());
    for policy in policies {
        if !policy.is_active {
            continue;
        }
        let Some(action) = AclAction::from_db_string(policy.action.as_str()) else {
            continue;
        };
        specs.push(AclPolicySpec {
            action,
            resource_type: policy.resource_type.clone(),
            resource_value: policy.resource_value.clone(),
            properties: policy.properties.clone(),
            mask_pattern: policy.mask_pattern.clone(),
            priority: policy.priority,
        });
    }
    AclSnapshot { policies: specs }
}

/// Scope the ACL snapshot + principal into the runtime task-locals
/// the Cypher pipeline reads, then await `fut`. Loads the snapshot
/// up-front so every Cypher-execution path gets the same wiring.
pub async fn with_acl_scope<F, T>(
    store: &dyn AclStore,
    principal: &Principal,
    ws: &WorkspaceContext,
    fut: F,
) -> Result<T, AppError>
where
    F: std::future::Future<Output = T>,
{
    use ox_runtime::{GRAPH_ACL_SNAPSHOT, GRAPH_PRINCIPAL};

    let snapshot = load_acl_snapshot(store, principal, ws).await?;
    let request_principal = request_principal(principal, ws);

    let inner = async move {
        if let Some(p) = request_principal {
            GRAPH_PRINCIPAL
                .scope(p, GRAPH_ACL_SNAPSHOT.scope(snapshot, fut))
                .await
        } else {
            GRAPH_ACL_SNAPSHOT.scope(snapshot, fut).await
        }
    };
    Ok(inner.await)
}

/// Apply an [`AclSnapshot`] to a materialised [`QueryResult`].
/// Used on the federation path: DataFusion executes a `LogicalPlan`
/// outside the Cypher pipeline, so the rewriter never sees it. Once
/// the federation path grows its own pre-execute pass we delete this.
pub fn enforce_acl_on_result(result: &mut QueryResult, snapshot: &AclSnapshot) {
    let mut deny: HashSet<&str> = HashSet::new();
    let mut mask: HashMap<&str, &str> = HashMap::new();
    for spec in &snapshot.policies {
        let Some(props) = &spec.properties else {
            continue;
        };
        match spec.action {
            AclAction::Deny => {
                for p in props {
                    deny.insert(p.as_str());
                }
            }
            AclAction::Mask => {
                let pattern = spec
                    .mask_pattern
                    .as_deref()
                    .unwrap_or("***");
                for p in props {
                    if !deny.contains(p.as_str()) {
                        mask.insert(p.as_str(), pattern);
                    }
                }
            }
        }
    }
    if deny.is_empty() && mask.is_empty() {
        return;
    }

    let mut deny_indices: Vec<usize> = Vec::new();
    let mut mask_indices: Vec<(usize, String)> = Vec::new();
    for (idx, col) in result.columns.iter().enumerate() {
        if deny.contains(col.as_str()) {
            deny_indices.push(idx);
        } else if let Some(pattern) = mask.get(col.as_str()) {
            mask_indices.push((idx, (*pattern).to_string()));
        }
    }

    for row in &mut result.rows {
        for (idx, pattern) in &mask_indices {
            if let Some(cell) = row.get_mut(*idx) {
                *cell = PropertyValue::String(pattern.clone());
            }
        }
    }

    if !deny_indices.is_empty() {
        deny_indices.sort_unstable();
        deny_indices.reverse();
        for idx in &deny_indices {
            result.columns.remove(*idx);
            for row in &mut result.rows {
                if *idx < row.len() {
                    row.remove(*idx);
                }
            }
        }
    }
}
