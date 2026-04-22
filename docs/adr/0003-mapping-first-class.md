# ADR 0003: Mapping (Object / Link / Property) as first-class concept

- Status: Accepted
- Date: 2026-04-20

## Context

In v1/v2 the binding between an ontology concept and a physical table was
implicit: `NodeTypeDef` carried a `source_lineage` string, and anything
more nuanced (one node type sourced from two systems, a bridge table, a
JSON-path extraction) was ad-hoc.

Real customer data does not fit that mold:

- A `Customer` node may be sourced from a legacy CRM table *and*
  Salesforce *and* a data-warehouse dimension.
- A `Contract → Customer` edge may be a bridge many-to-many, a direct
  foreign key, or a computed match across systems.
- A single property (`email`) may be `UPPER(src.email)` in one source
  and `LOWER(src.contact.email_address)` (JSON path) in another.

Industry precedent is clear — R2RML (W3C), Stardog Virtual Graphs,
dbt Semantic Layer, Denodo, and Palantir Foundry all elevate the
mapping to a first-class, versioned artifact separate from the logical
model.

## Decision

Mapping is a first-class concept in `ox-ontology`:

```rust
pub struct ObjectMappingDef {
    pub id: ObjectMappingId,
    pub node_type_id: NodeTypeId,
    pub source_id: SourceId,
    pub relation: SourceRelationKind,     // Table | View | Collection | File
    pub primary_key_columns: Vec<ColumnRef>,
    pub row_filter: Option<SqlExpr>,      // pushed into TableProvider scan
    pub property_mappings: Vec<PropertyMappingDef>,
    pub workspace_scope: Option<ColumnRef>,
    pub precedence: u8,                    // multi-mapping dedup ordering
    pub valid_from: Option<Timestamp>,
    pub valid_to: Option<Timestamp>,
    pub refresh_hint: CacheHintKind,
}

pub struct LinkMappingDef {
    pub id: LinkMappingId,
    pub edge_type_id: EdgeTypeId,
    pub kind: LinkMappingKind,             // ForeignKey | Bridge | Computed | Federated
    pub source_endpoint: EndpointRef,
    pub target_endpoint: EndpointRef,
    pub bridge_relation: Option<SourceRelationRef>,
    pub direction: EdgeDirection,
    pub join_cost_hint: JoinCostHint,
}

pub struct PropertyMappingDef {
    pub property_id: PropertyId,
    pub column_or_path: PropertyPathKind,  // ColumnRef | JsonPath
    pub transform: PropertyTransformKind,  // Identity | Rename | SqlExpr | JsonPath | Derived(FunctionId)
}
```

Semantics:

- **Multi-mapping on one node type** = `UNION ALL` at scan time, then
  `DISTINCT ON (primary_key_columns)` ordered by `precedence` descending.
  The highest-precedence row wins on conflict.
- **Interface targeting** (ADR: Part 2 of v3 model) expands to the union
  of implementing node types' mappings; interface-only properties are
  statically checked.
- **Federated edge** (`LinkMappingKind::Federated`) is the only edge
  whose endpoints live in different sources. The planner emits a
  bloom-filter hash join and surfaces a cost warning.
- **`row_filter` / `workspace_scope`** push into the `TableProvider::scan`
  as DataFusion filter expressions, never as string concatenation.
- **`valid_from` / `valid_to`** capture mapping lifetime. If a query's
  `ontology_valid_at` (ADR 0007) falls outside the mapping's window, the
  planner raises `MAPPING_NOT_VALID_AT` instead of silently returning
  wrong data.
- **R2RML import / export** is the standard interchange format; custom
  mapping DSLs are not introduced.

## Consequences

### Positive

- Multi-source ontologies are a native concept, not a hack.
- R2RML round-trip enables interoperability with any tool in that
  ecosystem without us owning a translation pipeline forever.
- The planner has a single authoritative resolver
  (`MappingResolver`) and does not have to interpret
  `source_lineage` strings.
- Mapping changes become audited events (`AuditEventDef`) and can
  trigger cache invalidation deterministically.

### Negative

- Mapping is now a UX surface (`MappingStudio`). It demands real
  tooling, and "just pick the right table" is not enough — users need
  guided editing, validation, and preview.
- The property-transform expression language is an opinion we own.
  We scope it narrowly (Identity / Rename / SqlExpr / JsonPath /
  Derived(FunctionId)); anything richer belongs in `FunctionDef`.

### Trade-offs

- We inherit R2RML's complexity in exchange for its interoperability.
  We do not adopt R2RML's RDF syntax internally; we use a Rust model
  and import/export against Turtle.

## Alternatives considered

1. **Keep mapping implicit on NodeTypeDef.** Rejected — fails at the
   first multi-source customer.
2. **Generate code (per-source Rust structs) at design time.** Rejected
   — brittle, re-compilation required for any remap, and defeats the
   "workspace live-edit" story.
3. **Adopt R2RML RDF as the internal representation.** Rejected — we
   would carry an RDF triplestore for metadata we already model better
   with typed Rust. We translate at the boundary instead.

## Related

- ADR 0001 — VOL as first-class execution.
- ADR 0002 — DataFusion federation engine.
- ADR 0006 — SHACL rules can target mappings (closed shapes).
- ADR 0010 — Canonical IRI scheme; mappings are IRI-identified.
