# ADR 0007: Bitemporal semantics — ontology-time and data-time

- Status: Accepted
- Date: 2026-04-20

## Context

The v1/v2 `TemporalRewriter` accepted a single `as_of` timestamp. That
timestamp conflated two independent axes:

1. **Ontology time** — which version of labels, mappings, and rules do
   we interpret the query against?
2. **Data time** — at what transaction / valid time of the source data
   should rows be read?

They are independent. A reasonable analytical question is:

> "Using the **current** customer-segmentation ontology, show me how
> revenue looked **last quarter**."

A single axis cannot express that; collapsing them is a correctness
bug hiding behind a pragmatic API.

Bitemporal databases (SQL:2011 `SYSTEM_TIME` and `BUSINESS_TIME`,
Snodgrass' bitemporal model) codified this distinction decades ago. The
semantic-web world separates `owl:versionIRI` (ontology time) from
named graph valid-time (data time). Palantir Foundry treats ontology
versioning and object versioning independently for the same reason.

## Decision

Queries carry two independent, optional timestamps:

```rust
pub struct QueryIR {
    // ... existing fields ...
    pub ontology_valid_at: Option<Timestamp>,
    pub data_valid_at:     Option<Timestamp>,
    // ...
}
```

Semantics:

- `ontology_valid_at`:
  - Selects which `OntologyDef` version is used for label resolution,
    mapping selection, rule application, and rename tracing.
  - `None` → the current committed version.
  - Resolved by `OntologyResolver` at stage 1 of the planner pipeline.
  - Drives `RenameRewriter` (previously `TemporalRewriter` — see
    ADR 0005) to translate current-vocabulary queries into the chosen
    snapshot's vocabulary.
- `data_valid_at`:
  - Pushed down to the source as an AS-OF clause where supported:
    - PostgreSQL with `pg_versioning`: `FOR SYSTEM_TIME AS OF $ts`
    - Snowflake: `AT (TIMESTAMP => $ts)`
    - BigQuery: snapshot decorator / `FOR SYSTEM_TIME AS OF`
    - DuckDB: version / snapshot where available
  - Unsupported sources raise `DATA_TIME_UNSUPPORTED_ON_SOURCE` with
    the source name; no silent fallback.
- A mapping's `valid_from` / `valid_to` must contain
  `ontology_valid_at`; otherwise the planner raises
  `MAPPING_NOT_VALID_AT`.
- The two axes may be combined freely. Neither implies the other.
- Temporal queries never serve from the graph cache unless the cache
  itself is bitemporal (Phase 7+); initial implementation routes
  temporal reads to source only.

## Consequences

### Positive

- Analytical questions that mix current ontology with historical data
  (or vice versa) are expressible correctly.
- Ontology-time isolates workbench edits: an in-flight ontology
  change does not invalidate a user's saved query, because the query
  pinned an `ontology_valid_at`.
- Data-time pushdown inherits source-level guarantees (system-time
  tables, snapshot isolation) rather than reinventing them.

### Negative

- Two optional fields mean four combinations users must understand.
  The `TimeTravel` UI panel presents them with a "current / pinned"
  toggle per axis rather than exposing raw timestamps by default.
- The graph cache cannot serve arbitrary `data_valid_at` until it
  becomes bitemporal (deferred).

### Trade-offs

- We trade API surface (one extra field, one extra planner stage) for
  semantic correctness on analytical workloads. The cost is flat; the
  benefit compounds as the ontology evolves.

## Alternatives considered

1. **Single `as_of` axis.** Rejected — incorrect for any query that
   separates "when did we think this" from "when was the data like
   that."
2. **Three axes (transaction time, valid time, ontology time).**
   Considered; rejected for Phase 1–8. Source-level transaction vs
   valid time is already handled by source-specific dialect; exposing
   a separate axis at the ontology level doubled the API surface for
   negligible use-case gain. Reopen if a concrete need arises.

## Related

- ADR 0003 — Mapping `valid_from` / `valid_to` interacts with
  `ontology_valid_at`.
- ADR 0004 — Graph cache temporarily bypassed for temporal reads.
- ADR 0008 — PROV-O provenance records both ontology and data times
  on each asserted fact.
