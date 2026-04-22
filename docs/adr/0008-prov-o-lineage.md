# ADR 0008: W3C PROV-O aligned provenance

- Status: Accepted
- Date: 2026-04-20

## Context

v1/v2 tracked lineage through ad-hoc `source_lineage` fields
(source id, table, primary key, last-seen timestamp). The fields
worked but were single-purpose. They answered "where did this row
come from?" and nothing else.

A mature ontology platform needs richer lineage:

- Which mapping produced this value at this timestamp?
- Which LLM prompt + model produced this ontology draft?
- Which user accepted which action, which derived property, which
  merge?
- Which validation report was produced by which rule at which time?
- How do we export lineage to SIEM / data-catalog tools?

W3C PROV-O (2013) is the answer the industry converged on: three
core types (Entity / Activity / Agent) with typed relations
(`wasGeneratedBy`, `used`, `wasAssociatedWith`, `wasDerivedFrom`,
`wasAttributedTo`). Data catalogs (DataHub, Amundsen, OpenLineage,
Atlan) ingest PROV-O dialects natively. W3C standardization means
it survives vendor churn.

## Decision

Ontosyx provenance aligns with PROV-O.

```rust
pub struct ProvenanceDef {
    pub id: ProvenanceId,
    pub entity_ref: AnyOntologyRef,                  // the thing whose origin we explain
    pub activity: ProvenanceActivityKind,            // what happened
    pub agent: AgentRef,                             // who (user | service | LLM model)
    pub derived_from: Vec<AnyOntologyRef>,           // prov:wasDerivedFrom
    pub used: Vec<AnyOntologyRef>,                   // prov:used
    pub at_time: Timestamp,                          // prov:atTime
    pub ontology_valid_at: Option<Timestamp>,
    pub data_valid_at: Option<Timestamp>,
    pub attributes: serde_json::Map<String, JsonValue>,
}

pub enum ProvenanceActivityKind {
    SourceScan    { source_id: SourceId, mapping_id: ObjectMappingId },
    FunctionEval  { function_id: FunctionId },
    RuleValidate  { rule_id: RuleId, outcome: ValidationOutcomeKind },
    ActionExecute { action_id: ActionId, idempotency_key: Option<String> },
    OntologyEdit  { command: OntologyCommandKind },
    DraftProposal { prompt_name: String, prompt_version: String, model: ResolvedModel },
    CacheRefresh  { mapping_id: ObjectMappingId },
    Enrichment    { enrichment_id: EnrichmentId },
    Import        { format: ImportFormatKind, uri: Option<Iri> },
    Export        { format: ExportFormatKind, uri: Option<Iri> },
}

pub enum AgentRef {
    User(UserId),
    Service(ServiceId),
    LlmModel(ResolvedModelRef),
    System,
}
```

Behavioural rules:

- Every mutation of an ontology artifact emits exactly one
  `ProvenanceDef` record. `ProvenanceTagger` (planner stage 13) also
  attaches provenance to query results — either inline on the
  `RecordBatch` via an extension column, or as a side channel in
  the streaming response envelope.
- The `AuditEventDef` log is a thin projection over `ProvenanceDef`;
  it is not a second source of truth.
- Provenance is **IRI-identified** (ADR 0010). Every
  `ProvenanceDef.id` can be serialized to a Turtle / JSON-LD document
  whose predicates match PROV-O exactly.
- Provenance records are append-only, workspace-scoped, RLS-enforced.
- Retention policy is per-workspace governance; default retention
  matches the workspace's audit retention.

## Consequences

### Positive

- Downstream catalogs ingest our provenance without a bespoke
  translator.
- Compliance review (SOX, GDPR Art. 30) can point at the provenance
  store directly.
- LLM traceability — every AI-generated draft carries its prompt +
  model + acceptance in the graph.
- Debugging regressions is deterministic: "what changed this node's
  address?" is a single provenance walk.

### Negative

- Provenance volume is high. A scan over a million rows produces a
  million row-level ancestries. We commit to **row-class** provenance
  (one record per `(mapping_id, query_run_id)` pair, not per row)
  with optional row-level attachments driven by
  `GovernanceDef.provenance_granularity`.
- Provenance export is a new surface area — Turtle + JSON-LD both —
  and we own the canonical form.

### Trade-offs

- We pay storage + throughput for traceability. The default
  granularity caps the cost at the query level; row-level provenance
  is opt-in per workspace.

## Alternatives considered

1. **Custom lineage record.** Rejected — permanent translation work.
2. **OpenLineage only.** Considered; OpenLineage overlaps with PROV-O
   but is narrower (job/run/dataset). We model PROV-O internally and
   map to OpenLineage on export.
3. **Blockchain / hash-chained log.** Not rejected outright — Phase 13
   may add hash-chain integrity on top of `ProvenanceDef`, but the
   base model is PROV-O regardless.

## Related

- ADR 0007 — Bitemporal timestamps flow through provenance records.
- ADR 0010 — Canonical IRI scheme provides provenance subject IDs.
- ADR 0005 — Agents and activities follow the `XxxRef` / `XxxKind`
  naming.
