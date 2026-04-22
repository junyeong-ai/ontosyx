//! Mapping layer — binds the logical ontology to physical data
//! sources.
//!
//! Three first-class definitions (ADR 0003):
//!
//! - [`ObjectMappingDef`] — one NodeType ↔ one physical relation.
//! - [`LinkMappingDef`] — one EdgeType ↔ the relation(s) that supply
//!   edges of that type (FK, bridge, computed, or federated).
//! - [`PropertyMappingDef`] — one property ↔ one value location
//!   (column / JSON path) plus an optional transform.
//!
//! Plus the reference types ([`SourceId`], [`ColumnRef`],
//! [`SourceRelationRef`], [`CacheHintKind`]) they share.
//!
//! Legacy [`legacy_source_mapping::SourceMapping`] survives for the
//! current design-flow path. It is a flat `HashMap`-backed shape
//! with none of the temporal / filter / precedence concepts the new
//! types carry. Callers are migrating onto the typed mappings; once
//! every consumer is updated the legacy type will be removed
//! (tracked in the Phase 4 migration plan).

pub mod legacy_source_mapping;
pub mod link;
pub mod object;
pub mod property;
pub mod refs;

#[allow(deprecated)]
pub use legacy_source_mapping::SourceMapping;
pub use link::{
    EndpointRef, JoinCostHint, LinkCardinality, LinkMappingDef, LinkMappingKind,
};
pub use object::ObjectMappingDef;
pub use property::{PropertyLocation, PropertyMappingDef, PropertyTransform};
pub use refs::{
    CacheHintKind, ColumnRef, LinkMappingId, ObjectMappingId, SourceId, SourceRelationKind,
    SourceRelationRef,
};
