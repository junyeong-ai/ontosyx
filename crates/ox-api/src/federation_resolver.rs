//! Helpers for the per-workspace federation adapter resolver.
//!
//! The federation surface keeps one [`WorkspaceResolverSlot`] slot per
//! workspace in `FederationState::federation_resolvers`. Each slot
//! holds a `OnceCell<RwLock<InMemoryAdapterResolver>>`:
//!
//! - **Hydration is singleflight** — the first access to a workspace
//!   runs the full `list_data_sources + build_adapter × N` routine
//!   exactly once per slot lifetime. Concurrent first-accesses
//!   (e.g., two federation queries arriving back-to-back on a fresh
//!   pod) await the same initialisation future instead of each
//!   rebuilding the adapter graph and paying the DB connect costs
//!   twice.
//! - **Post-hydration mutations** (admin register / delete) use the
//!   inner `RwLock` — the `OnceCell` commits only once per slot.
//! - **Refresh** drops the slot from the outer `DashMap` so the
//!   next access starts a fresh `OnceCell`. This is the one path
//!   that can "re-hydrate" a workspace; everything else mutates
//!   the existing resolver in place.
//!
//! Every entry point here takes `&FederationState` rather than
//! `&AppState` — the helpers need only the store, the resolver
//! map, and the secret resolver.
//!
//! Hydration itself is best-effort: a malformed stored row skips
//! with a `warn!`, other adapters in the same workspace still come
//! up. A dropped adapter behaves the same as "never registered",
//! which the resolver refuses with a descriptive error at query
//! time.

use std::sync::Arc;

use ox_federation::InMemoryAdapterResolver;
use ox_ontology::mapping::SourceId;
use ox_source::DataSourceAdapter;
use ox_store::PostgresStore;
use uuid::Uuid;

use crate::error::AppError;
use crate::routes::federation_admin::RegisterAdapterKind;
use crate::state::{FederationState, WorkspaceResolverSlot};

/// Outcome of [`upsert_workspace_adapter`]. `replaced` reports
/// whether a previous row for the same `source_id` existed in the
/// store at the moment of upsert — the check is performed inside
/// the same critical section as the upsert + register, so the
/// value is accurate w.r.t. the persisted truth even under
/// concurrent admin writes.
#[derive(Debug, Clone, Copy)]
pub struct AdapterUpsertOutcome {
    pub replaced: bool,
}

/// Return an owned `InMemoryAdapterResolver` snapshot for
/// `workspace_id`, hydrating from the `data_sources` store on cache
/// miss via singleflight [`WorkspaceResolverSlot::get_or_init`].
///
/// Callers receive a clone rather than a lock guard so they can
/// pass it to `ox_federation::build_query_ir_scoped(...)` without
/// holding the workspace's `RwLock` across the planner's async
/// `describe_table` calls. The clone is cheap — every adapter in
/// the resolver is behind an `Arc`.
pub async fn ensure_workspace_resolver(
    state: &FederationState,
    workspace_id: Uuid,
) -> Result<InMemoryAdapterResolver, AppError> {
    let slot = workspace_slot(state, workspace_id);
    let resolver_lock = slot
        .get_or_init(|| hydrate_workspace(state, workspace_id))
        .await?;
    Ok(resolver_lock.read().await.clone())
}

/// Register / replace the adapter for `workspace_id` / `source_id`
/// atomically — store write and in-memory write happen under the
/// same critical section.
///
/// Takes `&RegisterAdapterKind` rather than a pre-built adapter so
/// the build, the store upsert, and the in-memory register can all
/// live inside a single slot-level write lock. That closes a
/// TOCTOU race the previous split-write design exposed: with store
/// and memory writes happening in separate await points, two
/// concurrent registers of the same `source_id` could leave the
/// store and memory pinned to different versions (store → B,
/// memory → A) indefinitely. The critical section here serialises
/// concurrent registers for the same workspace, so the last write
/// wins in both locations atomically.
///
/// Hydration still runs once before the critical section (via
/// `OnceCell::get_or_init`) — a cold workspace gets its full
/// adapter graph before any single register mutates it, matching
/// the invariant that the in-memory resolver mirrors the store
/// after hydration.
///
/// The adapter itself is built OUTSIDE the critical section —
/// opening a Postgres connection pool or authenticating to
/// BigQuery can take seconds, and holding the workspace's resolver
/// lock across that would block all other admin writes for the
/// same workspace. The TOCTOU window only exists between store
/// upsert and memory register; that's what the lock covers.
pub async fn upsert_workspace_adapter(
    state: &FederationState,
    workspace_id: Uuid,
    source_id: &str,
    kind: &RegisterAdapterKind,
) -> Result<AdapterUpsertOutcome, AppError> {
    // Build outside the lock — slow DB / auth handshakes here do
    // not block other register paths.
    let adapter = kind.build_adapter(state.secret_resolver.as_ref()).await?;
    let config = kind.to_stored_config();

    let slot = workspace_slot(state, workspace_id);
    let resolver_lock = slot
        .get_or_init(|| hydrate_workspace(state, workspace_id))
        .await?;

    // --- Critical section --------------------------------------
    // Holds the resolver's write lock across three store/memory
    // operations. Concurrent registers for the same workspace
    // queue here; the store and the in-memory resolver always
    // see the same order of writes, which is what eliminates the
    // divergence race.
    let mut resolver = resolver_lock.write().await;

    let existing = state
        .store
        .find_data_source_by_source_id(source_id)
        .await
        .map_err(AppError::from)?;
    let replaced = existing.is_some();

    state
        .store
        .upsert_data_source_by_source_id(source_id, kind.kind_tag(), &config)
        .await
        .map_err(AppError::from)?;
    resolver.register(SourceId::new(source_id.to_string()), adapter);
    // --- end critical section ----------------------------------

    Ok(AdapterUpsertOutcome { replaced })
}

/// Remove the in-memory adapter for `workspace_id` / `source_id`.
/// Returns `true` when something was removed — mirrors the store's
/// `delete_data_source_by_source_id` return contract.
///
/// A cold workspace slot has no in-memory adapter to remove; returns
/// `false` without triggering hydration (there is no benefit to
/// hydrating just to delete something that isn't there).
pub async fn remove_workspace_adapter(
    state: &FederationState,
    workspace_id: Uuid,
    source_id: &str,
) -> bool {
    let Some(slot) = state.federation_resolvers.get(&workspace_id) else {
        return false;
    };
    let Some(resolver_lock) = slot.get() else {
        return false;
    };
    resolver_lock
        .write()
        .await
        .remove(&SourceId::new(source_id.to_string()))
        .is_some()
}

/// Rehydrate one workspace's resolver by replaying the store.
///
/// Drops the slot from the outer `DashMap` (so the next access
/// creates a fresh `OnceCell`), then calls [`ensure_workspace_resolver`]
/// to re-run hydration. Returns the number of adapters in the
/// rebuilt resolver so the admin endpoint can surface it as a
/// lightweight sanity check.
pub async fn refresh_workspace_resolver(
    state: &FederationState,
    workspace_id: Uuid,
) -> Result<usize, AppError> {
    state.federation_resolvers.remove(&workspace_id);
    let resolver = ensure_workspace_resolver(state, workspace_id).await?;
    Ok(resolver.len())
}

/// Get-or-insert the workspace's slot. The empty slot does nothing
/// by itself; `get_or_init` on it drives the actual hydration.
fn workspace_slot(state: &FederationState, workspace_id: Uuid) -> Arc<WorkspaceResolverSlot> {
    state
        .federation_resolvers
        .entry(workspace_id)
        .or_insert_with(|| Arc::new(WorkspaceResolverSlot::new()))
        .clone()
}

/// Build a fresh `InMemoryAdapterResolver` by replaying the
/// `data_sources` store under the workspace's RLS context.
///
/// Malformed rows skip with a `warn!` so one bad registration does
/// not take the whole workspace offline. The caller is
/// [`WorkspaceResolverSlot::get_or_init`], which pins the resolver
/// behind the slot's `OnceCell` once it returns.
async fn hydrate_workspace(
    state: &FederationState,
    workspace_id: Uuid,
) -> Result<InMemoryAdapterResolver, AppError> {
    let store = Arc::clone(&state.store);
    let rows = PostgresStore::with_workspace(workspace_id, || async move {
        store.list_data_sources().await
    })
    .await
    .map_err(AppError::from)?;

    let mut resolver = InMemoryAdapterResolver::new();
    for row in rows {
        match build_adapter_from_row(state, &row.kind, &row.config).await {
            Ok(adapter) => {
                resolver.register(SourceId::new(row.source_id.clone()), adapter);
            }
            Err(e) => {
                tracing::warn!(
                    workspace_id = %workspace_id,
                    source_id = %row.source_id,
                    kind = %row.kind,
                    error = ?e,
                    "federation_resolver: skipping stored data_source that failed to hydrate"
                );
            }
        }
    }
    Ok(resolver)
}

/// Build a live `DataSourceAdapter` from a persisted `data_sources`
/// row. Decodes the row into the typed [`RegisterAdapterKind`], then
/// defers to its `build_adapter` — the same code path the admin
/// register handler runs, so stored rows and fresh requests cannot
/// drift.
async fn build_adapter_from_row(
    state: &FederationState,
    kind: &str,
    config: &serde_json::Value,
) -> Result<Arc<dyn DataSourceAdapter>, AppError> {
    let decoded = RegisterAdapterKind::from_stored(kind, config)?;
    decoded.build_adapter(state.secret_resolver.as_ref()).await
}
