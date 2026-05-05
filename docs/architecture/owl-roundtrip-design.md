# OWL / Turtle round-trip — external ontology import + export

**Status:** Design sketch — Phase 10 of the long-horizon
work plan. The revised plan **defers full implementation
until first concrete user request** (FIBO / SNOMED /
schema.org / NeoSemantics import). Landing the design
sketch as `docs/architecture/owl-roundtrip-design.md`
captures the contract so the implementer has the trait
+ schema-mapping table + integration points without
re-deriving when the use case arrives.

## Demand gate

The earlier audit explicitly named OWL roundtrip as
"defer until requested" — Foundry / Stardog / TopBraid
ship it because their adopters arrive with FIBO /
SNOMED / schema.org corpora. Ontosyx adopters today
author from data-source introspection, not from a
pre-existing ontology import; the value of round-trip
lands when the first adopter brings a corpus.

So the v1 implementation ships with these gates:

- **Behind a `ox-export` workspace feature flag** —
  `workspaces.owl_roundtrip_enabled: bool`, default
  `false`. Operators opt in deliberately.
- **Endpoint surface gated on the flag** —
  `POST /api/ontology/owl/import` and
  `GET /api/ontology/owl/export` 404 unless the flag
  is set; admin UI surfaces the toggle.
- **Initial scope: schema-only** — TBox import / export.
  Instance data (ABox) round-trip is a separate
  follow-up; v1 ships a clean schema-shape boundary so
  the workspace's modelled concepts can interop with
  external tooling.

## Decision (sketch)

Two endpoints + one new crate. The crate (`ox-export`)
holds the OWL serialisation logic so the dependency
arrow stays clean — `ox-api → ox-export → ox-ontology`,
no edge from `ox-ontology` to RDF tooling.

### Crate: `ox-export`

```
crates/ox-export/
├── Cargo.toml          # rio_api / rio_turtle workspace deps
├── src/
│   ├── lib.rs          # public traits + re-exports
│   ├── owl/
│   │   ├── mod.rs
│   │   ├── exporter.rs # OntologyIR → Turtle
│   │   ├── importer.rs # Turtle → OntologyIR (best-effort)
│   │   └── prefixes.rs # canonical prefix registry
│   ├── shacl/          # OntologyIR.rules → SHACL Turtle
│   └── skos/           # OntologyIR.glossary → SKOS Turtle
└── tests/
    ├── fibo_subset_roundtrip.rs
    ├── schema_org_subset_import.rs
    └── snapshot_export.rs
```

The `rio_api` + `rio_turtle` workspace deps already
exist (per the workspace `Cargo.toml`); the crate
formalises their use.

### Public trait

```rust
pub trait OwlSerialiser: Send + Sync {
    /// Render an OntologyIR as a Turtle document. The
    /// emitted Turtle round-trips back through
    /// `OwlImporter::import_turtle` with no semantic
    /// loss for every IR feature listed in the
    /// "schema mapping table" below; features outside
    /// that table degrade to OWL2 `oboInOwl:Annotation`s
    /// the importer faithfully restores.
    fn export_turtle(&self, ontology: &OntologyIR) -> OxResult<String>;
}

pub trait OwlImporter: Send + Sync {
    /// Parse a Turtle document into an OntologyIR. The
    /// import is best-effort — features the IR doesn't
    /// natively model (OWL DL constructs, SWRL rules,
    /// inferred axioms) land as
    /// `OntologyIR.unmapped_axioms: Vec<UnmappedAxiom>`
    /// so the operator surface can flag them without
    /// data loss.
    fn import_turtle(&self, turtle: &str) -> OxResult<ImportResult>;
}

pub struct ImportResult {
    pub ontology: OntologyIR,
    pub unmapped_axioms: Vec<UnmappedAxiom>,
    pub warnings: Vec<ImportWarning>,
}
```

## Schema mapping table

The contract for "what round-trips losslessly":

| OntologyIR construct        | OWL / RDF representation                                  | Round-trip |
|-----------------------------|-----------------------------------------------------------|------------|
| `NodeTypeDef`               | `owl:Class`                                               | ✓ lossless |
| `EdgeTypeDef`               | `owl:ObjectProperty`                                       | ✓ lossless |
| `PropertyDef` (literal)     | `owl:DatatypeProperty`                                     | ✓ lossless |
| `PropertyDef` (entity ref)  | `owl:ObjectProperty`                                       | ✓ lossless |
| `InterfaceDef`              | `owl:Class` + `owl:equivalentClass` to union of impls    | ✓ lossless |
| `LinkCardinality`           | `owl:minCardinality` + `owl:maxCardinality` restrictions  | ✓ lossless |
| `ConceptDef`                | `skos:Concept` + `ontosyx:realisation` annotation         | ✓ lossless |
| `GlossaryTermDef`           | `skosxl:Label` + `skos:prefLabel` / `altLabel`             | ✓ lossless |
| `ShaclConstraint::*`        | `sh:NodeShape` + `sh:PropertyShape` (per ADR-0006)        | ✓ lossless |
| `ShaclConstraint::Or`       | `sh:or` rdf:List of nested constraints                    | ✓ lossless |
| `ProvenanceDef`             | PROV-O Turtle (per ADR-0008)                              | ✓ lossless |
| `MetricDef`                 | `ontosyx:Metric` annotation + expression literal           | ✗ lossy (expression-as-string) |
| `ActionDef`                 | `ontosyx:Action` annotation                                 | ✗ lossy (no OWL equivalent) |
| `SegmentDef`                | `ontosyx:Segment` annotation + body Turtle blob            | ✗ lossy (PatternIR-as-blob) |
| `UpperKind` (per ADR-0014)  | `bfo:` IRI ref (Object → `bfo:Object`, Event → `bfo:Process`, Agent → `bfo:Agent`, Concept → `skos:Concept`) | ✓ lossless |

The lossy entries land as `oboInOwl:Annotation`s with
the original JSON payload; the importer round-trips
them back into the IR but external OWL consumers see
opaque annotation strings. v2 of this surface might
formalise an OWL extension namespace
(`http://ontosyx.io/owl/`) for the platform-specific
constructs.

## Endpoint surface

```
GET  /api/ontology/owl/export         → 200 text/turtle
POST /api/ontology/owl/import         → 200 application/json
                                          { ontology, unmapped_axioms, warnings }
                                       → 422 application/json (typed error per ADR-0017)
                                          { code: "owl_import_parse_error", params: {...} }
```

Both routes gated on `principal.require_admin()` and
the workspace `owl_roundtrip_enabled` flag.

The import path is **draft-mode by default**: the
parsed `OntologyIR` lands as a new `OntologyDraft`
the operator reviews + commits through the existing
draft-completion pipeline. Direct import to canonical
is forbidden — the draft surface is the safety net
against importing a corpus that conflicts with the
workspace's existing concepts (per the ADR-0023 "no
auto decisions" invariant).

## NeoSemantics + n10s compatibility

A common adoption path is "operator already runs
Neo4j with `n10s` (NeoSemantics) and has imported
RDF graphs". The export's Turtle is `n10s`-compatible
out-of-the-box (uses standard `rdf:` / `owl:` /
`skos:` prefixes); the import path adds an optional
`format: "n10s_export"` parameter that handles the
`n10s.rdf.export.fetch` output shape (which embeds
some Neo4j-specific URIs).

## SPARQL endpoint stub

A SPARQL query endpoint (`POST /api/sparql/query`)
ships in v2 of this surface — out of scope for v1 but
the design space exists. The DataFusion VOL planner
can translate a SPARQL Basic Graph Pattern to the
existing `QueryIR::Match`; complex SPARQL features
(property paths, OPTIONAL, FILTER NOT EXISTS) need
the v2 work.

## Test pyramid

- **Unit tests** in `crates/ox-export/tests/`:
  - `fibo_subset_roundtrip.rs` — import a small FIBO
    Turtle subset, export it back, assert the canonical
    Turtle representation matches.
  - `schema_org_subset_import.rs` — import a
    schema.org `Person` / `Organization` subset, assert
    the `NodeTypeDef`s + `PropertyDef`s land correctly.
  - `snapshot_export.rs` — golden-file comparison of
    the canonical IR fixtures' Turtle export.
- **Integration test** in `crates/ox-api/tests/`:
  - `owl_endpoint_e2e.rs` — POST an import, assert it
    creates an `OntologyDraft` row, GET the export,
    assert the diff against the import is empty for
    losslessly-mapped constructs.
- **Compat fixture** — a `tests/fixtures/n10s_sample.ttl`
  file the importer's `format: "n10s_export"` path
  exercises.

## Out of scope (v1)

- **ABox (instance data) round-trip** — v1 ships
  TBox-only. Instance import would land entities into
  the federation graph; the existing data-source
  import flow already covers that path for relational
  sources, so the OWL ABox surface is a future
  decision.
- **OWL DL reasoning** — `owl:propertyChainAxiom`,
  `owl:transitiveProperty` reasoning. The IR's
  `InterfaceExpander` covers the dominant transitivity
  case; full DL reasoning is a Stardog-class feature
  that would need a tableau reasoner integration.
- **SPARQL query endpoint** — described above as v2.
- **Versioned import workflow** — re-importing the
  same Turtle into an existing draft (reconciling
  changes) is a Phase 11-class decision.

## References

- ADR-0001 — Virtual Ontology Layer (the federation
  surface SPARQL would compose with).
- ADR-0006 — SHACL as the rule model (the SHACL
  Turtle export reuses this contract).
- ADR-0008 — PROV-O lineage (the PROV-O Turtle
  export reuses this contract).
- ADR-0014 — `ConceptDef` (SKOS roundtrip target).
- ADR-0015 — `SegmentDef` (lossy export target).
- ADR-0017 — Typed error wire shape
  (`OwlImportParseError` typed code).
- ADR-0023 — `HeuristicProposal` (the import-
  reconciliation queue, for v1's draft-mode
  workflow).
- W3C OWL 2 — <https://www.w3.org/TR/owl2-overview/>
- W3C SKOS Core — <https://www.w3.org/TR/skos-reference/>
- W3C SHACL — <https://www.w3.org/TR/shacl/>
- W3C PROV-O — <https://www.w3.org/TR/prov-o/>
- NeoSemantics — <https://neo4j.com/labs/neosemantics/>
- Phase 10 of the long-horizon plan (deferred until
  first user request).
