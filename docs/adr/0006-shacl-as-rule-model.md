# ADR 0006: SHACL Core as the rule model

- Status: Accepted
- Date: 2026-04-20

## Context

The v2 sketch introduced a `BusinessRule` enum with four ad-hoc variants
(`EdgeCardinality`, `DerivedProperty`, `CrossEntityConstraint`,
`StateMachine`). In review the shape looked clean, but on second pass
three problems appeared.

1. **Reinvention.** SHACL (Shapes Constraint Language, W3C 2017) already
   models every constraint in that enum plus datatype, pattern,
   enumeration, closed shapes, disjoint properties, node shapes vs
   property shapes, and so on. Designing a custom enum means tracking
   SHACL's evolution by hand forever.
2. **Blurred responsibilities.** `DerivedProperty` is not a constraint
   — it is a *derivation*. Putting it under `BusinessRule` confuses
   validation with computation. `FunctionDef` is the right home for
   derivations.
3. **No interoperability story.** A customer importing an existing
   SHACL shape library could not map it into Ontosyx; the enum would
   absorb only the subset we happened to anticipate.

Interfaces to SHACL implementations are plentiful (`pyshacl`,
`shacl-rs`, Stardog ICV, TopBraid), so adopting it as the rule model
gives us a validator ecosystem for free.

## Decision

**SHACL Core is the rule model.** `RuleDef` is the SHACL shape; the
`BusinessRule` enum is deleted.

```rust
pub struct RuleDef {
    pub id: RuleId,
    pub iri: Iri,
    pub kind: RuleKind,
    pub severity: Severity,                 // Violation | Warning | Info
    pub enforcement: EnforcementKind,       // Write | Read | Batch
    pub activation: RuleActivationKind,     // Always | OnAction(ActionId) | OnSchedule(Cron)
    pub body: RuleBody,
}

pub enum RuleKind {
    NodeShape,       // sh:NodeShape targeting a node type
    PropertyShape,   // sh:PropertyShape on a specific property
    EdgeShape,       // Ontosyx extension: shape on an edge type
    CrossEntityShape,// uses sh:sparql to assert across nodes
    StateMachine,    // Ontosyx extension: valid transitions for a state property
}

pub struct RuleBody {
    pub target: RuleTarget,                 // class / node / property / sparql
    pub constraints: Vec<ShaclConstraint>,  // minCount, maxCount, datatype, pattern, in, hasValue,
                                            // minInclusive, maxInclusive, minLength, maxLength,
                                            // uniqueLang, closed, disjoint, ...
}
```

Semantics:

- `kind ∈ {NodeShape, PropertyShape}` = plain SHACL Core, compiles to
  `sh:*` constraints 1:1.
- `kind = EdgeShape` = Ontosyx extension, compiled as a `NodeShape` on
  a synthesized edge class when exported to Turtle.
- `kind = CrossEntityShape` = SHACL-SPARQL target. We compile to a
  DataFusion SQL assertion for federation; we export to `sh:sparql`
  for interchange.
- `kind = StateMachine` = Ontosyx extension, compiles to a disjunction
  of `sh:in` constraints keyed on the previous value. Enforced by
  `RuleValidator` at `Write` time.
- `enforcement = Write` = checked in `QueryPlanner` pre-execute on
  mutations.
- `enforcement = Read` = checked during result shaping; violations
  surface as `ResultIssue` not as an execution error.
- `enforcement = Batch` = evaluated by a scheduled reconciler, stored
  as a `DataQualityReport`.

## Consequences

### Positive

- Customers can import SHACL shape libraries directly.
- OWL / Turtle export of rules is mechanical, not bespoke (ADR 0008 on
  PROV-O + Canonical IRI composes with this).
- The enforcement-time distinction (Write / Read / Batch) is explicit;
  the same rule can attach to any of them without rewriting.
- Tooling: SHACL constraint catalogs, visualisers, test suites.
- Validation diagnostics align with SHACL's own `sh:ValidationReport`
  shape, so the wire format is industry-standard.

### Negative

- SHACL has sharp edges (`sh:and` / `sh:or` / `sh:xone`, property
  paths, recursion rules). We commit to a subset — **SHACL Core
  constraint components** — and reject advanced shapes at import time
  with a clear error.
- `sh:sparql` requires a SPARQL engine to validate literally. We do
  not ship one; we translate to DataFusion SQL where the shape is
  expressible, and reject with `SHAPE_UNSUPPORTED_SPARQL` otherwise.
- Authors unfamiliar with SHACL face a learning curve. Mitigated by
  `RuleStudio` UI that hides the vocabulary behind form controls.

### Trade-offs

- We accept SHACL's verbosity in exchange for its standardisation.
- We accept a Core-only profile in Phase 5-B; advanced SHACL features
  are tracked in Phase 11 (`ox-inference`).

## Alternatives considered

1. **Keep the custom enum.** Rejected — no interop, no ecosystem,
   permanent maintenance.
2. **OCL (Object Constraint Language).** Rejected — UML-centric,
   tooling is concentrated in the Eclipse ecosystem, JVM-heavy.
3. **Datalog / Vadalog.** Retained as a Phase 11 reasoning option; not
   a fit for simple shape constraints because it demands an inference
   engine, not a validator.
4. **JSON Schema.** Rejected for ontology rules — JSON Schema does not
   reason about cross-entity constraints, cardinality on relationships,
   or closed shapes over an RDF-shaped graph.

## Related

- ADR 0005 — `BusinessRule` enum deletion, `Severity` rename.
- ADR 0008 — PROV-O and SHACL compose for validation provenance.
- ADR 0010 — Canonical IRI scheme for `sh:NodeShape` targets.
