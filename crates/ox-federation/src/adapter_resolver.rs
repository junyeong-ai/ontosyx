//! `AdapterResolver` — map a `SourceId` to a live
//! `DataSourceAdapter` instance.
//!
//! Mappings reference sources by opaque id
//! (`ObjectMappingDef::source_id`). When the planner is ready to
//! turn a scan into a DataFusion `LogicalPlan` it needs the *actual*
//! adapter implementation to wrap in a `SourceTableProvider`. That
//! lookup is the `AdapterResolver` trait.
//!
//! In production (ox-api) the resolver is backed by a
//! workspace-scoped adapter registry — the same registry that owns
//! connection pools and lifecycle. In tests and bring-up paths the
//! [`InMemoryAdapterResolver`] is enough; it keeps the planner
//! source-agnostic and lets Phase 2's CSV infrastructure be reused
//! by the end-to-end federation tests.

use std::collections::HashMap;
use std::sync::Arc;

use ox_ontology::mapping::SourceId;
use ox_source::DataSourceAdapter;

use crate::error::{FederationError, FederationResult};

/// Resolve a `SourceId` to a ready-to-scan adapter.
///
/// Trait-based so ox-api can plug its workspace-aware registry in
/// without dragging that machinery into this crate. Phase 6-C slice 2
/// only needs the synchronous shape; later slices that need
/// lazy / async adapter construction (e.g. Snowflake auth
/// handshake) can add an async companion method.
pub trait AdapterResolver: Send + Sync {
    /// Return the adapter registered under `source_id`. Returns
    /// `FederationError::Unsupported` with a descriptive message
    /// when no such adapter exists — the planner treats that as a
    /// plan-time rejection rather than a runtime error.
    fn resolve(&self, source_id: &SourceId) -> FederationResult<Arc<dyn DataSourceAdapter>>;
}

/// HashMap-backed resolver. Intended for tests and for bring-up
/// scenarios (e.g. the `/api/query/sql` harness) where the adapter
/// set is known up-front and fits in memory.
///
/// `Clone` is derived — every field is cheap (Arcs over adapter
/// trait objects). Cloning produces a snapshot that shares backing
/// adapters with the original, so downstream planners can hold an
/// owned copy without keeping an outer map lock pinned.
#[derive(Clone)]
pub struct InMemoryAdapterResolver {
    adapters: HashMap<SourceId, Arc<dyn DataSourceAdapter>>,
}

impl std::fmt::Debug for InMemoryAdapterResolver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InMemoryAdapterResolver")
            .field("adapter_count", &self.adapters.len())
            .finish_non_exhaustive()
    }
}

impl InMemoryAdapterResolver {
    pub fn new() -> Self {
        Self {
            adapters: HashMap::new(),
        }
    }

    /// Register (or replace) the adapter for `source_id`.
    pub fn register(
        &mut self,
        source_id: impl Into<SourceId>,
        adapter: Arc<dyn DataSourceAdapter>,
    ) {
        self.adapters.insert(source_id.into(), adapter);
    }

    pub fn len(&self) -> usize {
        self.adapters.len()
    }

    pub fn is_empty(&self) -> bool {
        self.adapters.is_empty()
    }

    /// Remove the adapter registered under `source_id`. Returns
    /// `None` when nothing was registered — callers surface that as
    /// a 404 at the HTTP layer.
    pub fn remove(&mut self, source_id: &SourceId) -> Option<Arc<dyn DataSourceAdapter>> {
        self.adapters.remove(source_id)
    }

    /// Snapshot of `(source_id, source_type)` pairs currently in the
    /// registry. Consumers render this in admin UIs / list endpoints;
    /// the method returns an owned `Vec` so the caller does not hold
    /// a read guard while serialising.
    pub fn descriptions(&self) -> Vec<(SourceId, String)> {
        self.adapters
            .iter()
            .map(|(id, adapter)| (id.clone(), adapter.source_type().to_string()))
            .collect()
    }
}

impl Default for InMemoryAdapterResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl AdapterResolver for InMemoryAdapterResolver {
    fn resolve(&self, source_id: &SourceId) -> FederationResult<Arc<dyn DataSourceAdapter>> {
        self.adapters
            .get(source_id)
            .cloned()
            .ok_or_else(|| {
                FederationError::unsupported(format!(
                    "AdapterResolver: no adapter registered for source '{source_id}'"
                ))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ox_source::sample::CsvAdapter;

    #[test]
    fn empty_resolver_returns_unsupported() {
        // `Arc<dyn DataSourceAdapter>` does not implement `Debug`,
        // so `expect_err` — which requires the `Ok` type to be
        // `Debug` — is unavailable. Match the result instead.
        let r = InMemoryAdapterResolver::new();
        assert!(r.is_empty());
        match r.resolve(&SourceId::new("pg-main")) {
            Err(FederationError::Unsupported(_)) => {}
            Err(other) => panic!("expected Unsupported, got {other}"),
            Ok(_) => panic!("empty resolver must not return an adapter"),
        }
    }

    #[test]
    fn registered_adapter_round_trips_through_resolve() {
        let mut r = InMemoryAdapterResolver::new();
        let adapter: Arc<dyn DataSourceAdapter> =
            Arc::new(CsvAdapter::new("id,name\n1,a\n").unwrap());
        r.register("csv-1", adapter);
        assert_eq!(r.len(), 1);
        let back = r.resolve(&SourceId::new("csv-1")).unwrap();
        assert_eq!(back.source_type(), "csv");
    }

    #[test]
    fn register_replaces_existing_entry() {
        let mut r = InMemoryAdapterResolver::new();
        let a1: Arc<dyn DataSourceAdapter> =
            Arc::new(CsvAdapter::new("id\n1\n").unwrap());
        let a2: Arc<dyn DataSourceAdapter> =
            Arc::new(CsvAdapter::new("x,y\n1,2\n").unwrap());
        r.register("csv-1", a1);
        r.register("csv-1", a2);
        assert_eq!(r.len(), 1);
    }
}
