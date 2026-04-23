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
//! The three `XxxDef` types above are the canonical mapping layer;
//! the `object_mappings` vector on [`crate::ir::OntologyIR`] is the
//! single source of truth for "which source relation supplies a given
//! node type". Earlier flat-HashMap `SourceMapping` and the transitional
//! `ObjectMappingLookup` trait have been removed.

pub mod link;
pub mod object;
pub mod property;
pub mod refs;

pub use link::{
    EndpointRef, JoinCostHint, LinkCardinality, LinkMappingDef, LinkMappingKind,
};
pub use object::ObjectMappingDef;
pub use property::{PropertyLocation, PropertyMappingDef, PropertyTransform};
pub use refs::{
    CacheHintKind, ColumnRef, LinkMappingId, ObjectMappingId, SourceId, SourceRelationKind,
    SourceRelationRef,
};
