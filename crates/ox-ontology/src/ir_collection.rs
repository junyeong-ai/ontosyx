//! [`IrCollection`] — type-system contract that every typed `*Def`
//! collection in [`crate::OntologyIR`] must implement to participate
//! in the content-addressed storage layer + retrieval indexes.
//!
//! ## Why a trait
//!
//! `extract_entities` (the IR → Level-2 store extractor in
//! [`crate::storage`]) used to hand-write a `for nt in
//! ir.node_types() { extract(EntityKind::NodeType, &nt.id, nt) }`
//! loop per collection. The pattern was uniform — kind + logical
//! id + serialise — but the wiring was scattered across ~20 lines.
//! Adding a new collection (Φ8.2: [`crate::SegmentDef`] +
//! [`crate::table_inventory::TableInventoryEntry`]) required
//! remembering to add the loop AND the matching `EntityKind`
//! variant AND the `as_str` arm AND the `parse` arm — four
//! independent edits, any one of which silently regresses
//! retrieval if forgotten.
//!
//! Promoting the contract to a trait collapses the wiring:
//!
//! - The trait carries `const ENTITY_KIND` so the kind comes from
//!   the type, not the loop body.
//! - The trait carries `fn logical_id(&self)` so composite ids
//!   ([`crate::table_inventory::TableInventoryEntry`] is keyed
//!   on `(source_id, table_name)`) live next to the type they
//!   describe.
//! - `extract_entities` reduces to a single generic helper called
//!   N times — one line per collection in the IR root.
//!
//! ## Future extension
//!
//! When new IR collections land, the trait impl + the extractor
//! line are the entire mechanical surface. The `EntityKind` enum
//! still requires manual variant + `as_str` + `parse` arms (the
//! Postgres ENUM is the boundary — a derive-macro could fold the
//! enum too, but the compile-time benefit is small versus the
//! proc-macro maintenance cost).

use std::borrow::Cow;

use crate::storage::EntityKind;

/// Every collection that the content-addressed store extracts as
/// a top-level entity implements `IrCollection`. Bound on
/// `serde::Serialize` so the canonicaliser can render the value
/// without per-impl glue.
pub trait IrCollection: serde::Serialize {
    /// The `EntityKind` variant for this collection. Stays a
    /// const because every instance of a given type maps to the
    /// same kind — no instance-level dispatch.
    const ENTITY_KIND: EntityKind;

    /// Stable string identifier within the workspace's ontology.
    /// Single-id types borrow from their `id` field; composite
    /// keys synthesise via `Cow::Owned` (e.g.
    /// [`crate::table_inventory::TableInventoryEntry`] joins
    /// `source_id` + `table_name`).
    fn logical_id(&self) -> Cow<'_, str>;
}

// ---------------------------------------------------------------------------
// Topology
// ---------------------------------------------------------------------------

impl IrCollection for crate::ir::NodeTypeDef {
    const ENTITY_KIND: EntityKind = EntityKind::NodeType;
    fn logical_id(&self) -> Cow<'_, str> {
        Cow::Borrowed(self.id.as_str())
    }
}

impl IrCollection for crate::ir::EdgeTypeDef {
    const ENTITY_KIND: EntityKind = EntityKind::EdgeType;
    fn logical_id(&self) -> Cow<'_, str> {
        Cow::Borrowed(self.id.as_str())
    }
}

impl IrCollection for crate::ir::IndexDef {
    const ENTITY_KIND: EntityKind = EntityKind::IndexDef;
    fn logical_id(&self) -> Cow<'_, str> {
        match self {
            crate::ir::IndexDef::Single { id, .. }
            | crate::ir::IndexDef::Composite { id, .. }
            | crate::ir::IndexDef::FullText { id, .. }
            | crate::ir::IndexDef::Vector { id, .. } => Cow::Borrowed(id.as_str()),
        }
    }
}

impl IrCollection for crate::interface::InterfaceDef {
    const ENTITY_KIND: EntityKind = EntityKind::Interface;
    fn logical_id(&self) -> Cow<'_, str> {
        Cow::Borrowed(self.id.as_str())
    }
}

// ---------------------------------------------------------------------------
// Mapping
// ---------------------------------------------------------------------------

impl IrCollection for crate::mapping::ObjectMappingDef {
    const ENTITY_KIND: EntityKind = EntityKind::ObjectMapping;
    fn logical_id(&self) -> Cow<'_, str> {
        Cow::Borrowed(self.id.as_str())
    }
}

impl IrCollection for crate::mapping::LinkMappingDef {
    const ENTITY_KIND: EntityKind = EntityKind::LinkMapping;
    fn logical_id(&self) -> Cow<'_, str> {
        Cow::Borrowed(self.id.as_str())
    }
}

// ---------------------------------------------------------------------------
// Governance
// ---------------------------------------------------------------------------

impl IrCollection for crate::rule::RuleDef {
    const ENTITY_KIND: EntityKind = EntityKind::Rule;
    fn logical_id(&self) -> Cow<'_, str> {
        Cow::Borrowed(self.id.as_str())
    }
}

impl IrCollection for crate::data_quality::DataQualityDef {
    const ENTITY_KIND: EntityKind = EntityKind::DataQuality;
    fn logical_id(&self) -> Cow<'_, str> {
        Cow::Borrowed(self.id.as_str())
    }
}

impl IrCollection for crate::action::ActionDef {
    const ENTITY_KIND: EntityKind = EntityKind::Action;
    fn logical_id(&self) -> Cow<'_, str> {
        Cow::Borrowed(self.id.as_str())
    }
}

impl IrCollection for crate::provenance::ProvenanceDef {
    const ENTITY_KIND: EntityKind = EntityKind::Provenance;
    fn logical_id(&self) -> Cow<'_, str> {
        Cow::Borrowed(self.id.as_str())
    }
}

// ---------------------------------------------------------------------------
// Behaviour
// ---------------------------------------------------------------------------

impl IrCollection for crate::function::FunctionDef {
    const ENTITY_KIND: EntityKind = EntityKind::Function;
    fn logical_id(&self) -> Cow<'_, str> {
        Cow::Borrowed(self.id.as_str())
    }
}

impl IrCollection for crate::metric::MetricDef {
    const ENTITY_KIND: EntityKind = EntityKind::Metric;
    fn logical_id(&self) -> Cow<'_, str> {
        Cow::Borrowed(self.id.as_str())
    }
}

impl IrCollection for crate::enrichment::EnrichmentDef {
    const ENTITY_KIND: EntityKind = EntityKind::Enrichment;
    fn logical_id(&self) -> Cow<'_, str> {
        Cow::Borrowed(self.id.as_str())
    }
}

// ---------------------------------------------------------------------------
// Vocabulary + value semantics
// ---------------------------------------------------------------------------

impl IrCollection for crate::concept::ConceptDef {
    const ENTITY_KIND: EntityKind = EntityKind::Concept;
    fn logical_id(&self) -> Cow<'_, str> {
        Cow::Borrowed(self.id.as_str())
    }
}

impl IrCollection for crate::glossary::GlossaryTermDef {
    const ENTITY_KIND: EntityKind = EntityKind::GlossaryTerm;
    fn logical_id(&self) -> Cow<'_, str> {
        Cow::Borrowed(self.id.as_str())
    }
}

impl IrCollection for crate::code_system::CodeSystemDef {
    const ENTITY_KIND: EntityKind = EntityKind::CodeSystem;
    fn logical_id(&self) -> Cow<'_, str> {
        Cow::Borrowed(self.id.as_str())
    }
}

impl IrCollection for crate::value_set::ValueSetDef {
    const ENTITY_KIND: EntityKind = EntityKind::ValueSet;
    fn logical_id(&self) -> Cow<'_, str> {
        Cow::Borrowed(self.id.as_str())
    }
}

impl IrCollection for crate::notation_pattern::NotationPatternDef {
    const ENTITY_KIND: EntityKind = EntityKind::NotationPattern;
    fn logical_id(&self) -> Cow<'_, str> {
        Cow::Borrowed(self.id.as_str())
    }
}

impl IrCollection for crate::concept_map::ConceptMapDef {
    const ENTITY_KIND: EntityKind = EntityKind::ConceptMap;
    fn logical_id(&self) -> Cow<'_, str> {
        Cow::Borrowed(self.id.as_str())
    }
}

impl IrCollection for crate::value_range::ValueRangeSetDef {
    const ENTITY_KIND: EntityKind = EntityKind::ValueRangeSet;
    fn logical_id(&self) -> Cow<'_, str> {
        Cow::Borrowed(self.id.as_str())
    }
}

impl IrCollection for crate::column_profile::ColumnProfileDef {
    const ENTITY_KIND: EntityKind = EntityKind::ColumnProfile;
    fn logical_id(&self) -> Cow<'_, str> {
        Cow::Borrowed(self.id.as_str())
    }
}

// ---------------------------------------------------------------------------
// Φ8.2 additions — previously missing from extraction
// ---------------------------------------------------------------------------

impl IrCollection for crate::segment::SegmentDef {
    const ENTITY_KIND: EntityKind = EntityKind::Segment;
    fn logical_id(&self) -> Cow<'_, str> {
        Cow::Borrowed(self.id.as_str())
    }
}

impl IrCollection for crate::table_inventory::TableInventoryEntry {
    const ENTITY_KIND: EntityKind = EntityKind::TableInventory;
    fn logical_id(&self) -> Cow<'_, str> {
        // Composite natural key — `(source_id, table_name)` is what
        // `add_table_inventory_entry` upserts on, so the logical id
        // mirrors that pair. Owned because the format string crosses
        // the borrow.
        Cow::Owned(format!("{}:{}", self.source_id.as_str(), self.table_name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entity_kind_is_constant_per_type() {
        // Pin a representative sample so a careless impl edit
        // (e.g. copy-pasting `NodeType` into `EdgeTypeDef`) trips
        // a build assertion.
        assert_eq!(
            <crate::ir::NodeTypeDef as IrCollection>::ENTITY_KIND,
            EntityKind::NodeType
        );
        assert_eq!(
            <crate::ir::EdgeTypeDef as IrCollection>::ENTITY_KIND,
            EntityKind::EdgeType
        );
        assert_eq!(
            <crate::segment::SegmentDef as IrCollection>::ENTITY_KIND,
            EntityKind::Segment
        );
        assert_eq!(
            <crate::table_inventory::TableInventoryEntry as IrCollection>::ENTITY_KIND,
            EntityKind::TableInventory
        );
    }

    #[test]
    fn table_inventory_logical_id_joins_source_and_table() {
        let entry = crate::table_inventory::TableInventoryEntry::imported(
            crate::mapping::SourceId::new("pg-main"),
            "customers",
            "fp-1",
            vec![],
        );
        assert_eq!(entry.logical_id(), "pg-main:customers");
    }
}
