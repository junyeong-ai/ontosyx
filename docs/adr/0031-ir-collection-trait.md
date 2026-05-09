# ADR-0031 — `IrCollection` trait + EntityKind extraction contract

## Status

Accepted — Φ8.2, 2026-05-08.

## Context

`extract_entities` (`crates/ox-ontology/src/storage.rs`) walks
`OntologyIR` and emits one `ExtractedEntity` per top-level
collection member. The pre-Φ8.2 implementation hand-wrote one
`for-extract` loop per collection:

```rust
for nt in ir.node_types() {
    out.push(extract(EntityKind::NodeType, &nt.id, nt)?);
}
for et in ir.edge_types() {
    out.push(extract(EntityKind::EdgeType, &et.id, et)?);
}
// … 18 more loops …
```

The pattern was uniform — kind + logical id + serialise — but
the wiring was scattered. Adding a new collection required four
independent edits, each in a different file:

1. `EntityKind` enum variant
2. `EntityKind::as_str` arm
3. `EntityKind::parse` arm (the wire-string round-trip gate
   tested by `entity_kind_wire_names_round_trip_through_parse`)
4. `extract_entities` loop body

A repository audit (Agent A, 2026-05-08) found that
`SegmentDef` (ADR-0015) and `TableInventoryEntry` had IR-level
collections + validation rules but were missing from
`extract_entities`. The four-edit shape made it easy to forget;
the symptom was silent — retrieval / search anchored on a
segment's logical id returned zero hits because no row existed
in the content-addressed store. P0 from the audit.

## Decision

Promote the per-collection contract into a typed trait.

**`IrCollection`** (`crates/ox-ontology/src/ir_collection.rs`):

```rust
pub trait IrCollection: serde::Serialize {
    const ENTITY_KIND: EntityKind;
    fn logical_id(&self) -> std::borrow::Cow<'_, str>;
}
```

- `const ENTITY_KIND` — the variant lives on the type, not in
  the loop body. Adding a new collection without an
  `IrCollection` impl is a compile error in `extract_collection`
  (the generic helper requires the bound).
- `fn logical_id` returns `Cow<'_, str>` so single-id types
  (`NodeTypeDef.id` newtypes deref to `&str`) borrow zero-copy
  while composite-key types
  (`TableInventoryEntry { source_id, table_name }`) synthesise
  `Cow::Owned(format!("{source}:{table}"))` next to the type
  they describe.

**`extract_collection`** (`crates/ox-ontology/src/storage.rs`)
collapses every per-collection loop to one line:

```rust
extract_collection(&mut out, ir.node_types())?;
extract_collection(&mut out, ir.edge_types())?;
extract_collection(&mut out, ir.segments())?;          // Φ8.2
extract_collection(&mut out, ir.table_inventory())?;   // Φ8.2
```

22 collections now share the helper. The header entity stays
inline (it has no enclosing collection — singleton per IR).

`EntityKind` gains two variants:

- `Segment` — wire string `"segment"`, mirrored in the
  `ontology_entity_kind` Postgres ENUM.
- `TableInventory` — wire string `"table_inventory"`.

Both arms land in `as_str` + `parse`; the exhaustive match in
`every_variant_appears_in_all_variants` (storage tests) catches
a future variant added without a wire string.

The `assemble_ir` hydration path
(`crates/ox-store/src/postgres/ontology_materialize.rs`) gains
the symmetric arms — Segment + TableInventory rows fan back
into `ir.add_segment` / `ir.upsert_table_inventory_entry` so
the round-trip is closed end-to-end.

## Consequences

- **Adding a collection is a 2-edit change.** Author writes
  `impl IrCollection for FooDef { … }` and adds one
  `extract_collection(&mut out, ir.foos())?;` line. The
  `EntityKind` enum + ENUM wire string are still manual (the
  Postgres boundary needs them), but the generic helper's bound
  catches the omission at compile time.
- **Composite logical ids land naturally.**
  `TableInventoryEntry` is keyed on `(source_id, table_name)`;
  the `Cow::Owned(format!(…))` impl puts the synthesis next to
  the type it describes rather than embedded in an unrelated
  loop body.
- **Audit-found gaps fixed.** Segment + TableInventory now
  participate in content-addressed storage, hydration, and the
  Level-3 search / navigation indexes (the
  `materialize_level3` path keys on `EntityKind`).
- **Historical entities re-materialise on next commit.** The
  schema baseline added the two ENUM variants; existing
  workspaces that boot against the updated baseline pick up
  segment + table-inventory rows the next time their canonical
  ontology is committed. No data migration needed because the
  baseline assumes greenfield deployment per the wider Φ8
  no-backward-compat decision.

## Alternatives considered

- **Proc-macro derive (`#[derive(IrCollection)]`).** Rejected
  for now — the `const ENTITY_KIND` + `logical_id` pair is
  short enough to write by hand (4 lines per impl), and the
  proc-macro crate adds maintenance + compile-time cost without
  proportional value. If the IR grows past ~40 collections, a
  derive becomes worth the cost; today it isn't.
- **Embed extraction state in `OntologyIR` itself
  (`pub fn extract_entities(&self)`).** Rejected — keeps the
  domain layer free of storage concerns. The current placement
  at `crates/ox-ontology/src/storage.rs` mirrors the layered
  separation: domain types in `ir/`, the storage extractor +
  trait in a sibling module that consumes them.
- **Skip the trait, add a derive macro that generates the
  `extract_entities` body wholesale.** Rejected — same
  proc-macro maintenance cost, weaker compile-time signal
  (a missing `IrCollection` impl would be flagged late, only
  on the macro-generated body's expansion).

## Migration

Schema baseline `0001_schema.sql` adds two values to the
`ontology_entity_kind` ENUM (`segment`, `table_inventory`).
Greenfield deployment assumed; an existing workspace would need
`ALTER TYPE ontology_entity_kind ADD VALUE 'segment'` /
`'table_inventory'` (PG ALTER ENUM is non-transactional but
otherwise straightforward).

## References

- `crates/ox-ontology/src/ir_collection.rs` — trait + 22 impls
- `crates/ox-ontology/src/storage.rs` —
  `extract_collection` + `extract_entities`
- `crates/ox-store/migrations/0001_schema.sql` — ENUM variants
- `crates/ox-store/src/postgres/ontology_materialize.rs` —
  hydration arms
- ADR-0014 — `ConceptDef` as canonical identity (Concept
  variant already present; cross-reference)
- ADR-0015 — `SegmentDef` as first-class IR collection (the
  ADR this storage gap regressed against)
