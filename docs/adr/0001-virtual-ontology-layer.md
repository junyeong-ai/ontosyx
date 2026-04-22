# ADR 0001: Virtual Ontology Layer as first-class execution model

- Status: Accepted
- Date: 2026-04-20

## Context

The v1/v2 architecture assumed customer data would be migrated into a graph
database (Neo4j / Memgraph) and every query would run there. Two problems
follow from that assumption.

1. **Migration is the dominant failure mode.** Enterprise customers do not
   move PostgreSQL / Snowflake / BigQuery tables into a graph DB. They need
   their data where it already is, with the existing RLS / audit / backup
   tooling still applying. ETL into a graph DB invents a second source of
   truth, a CDC debt, and a governance gap.

2. **The industry has already solved this.** OBDA (Ontology-Based Data
   Access) / Virtual Knowledge Graph / Data Virtualization are mature
   patterns — Denodo, Stardog Virtual Graphs, Ontop, Starburst, Dremio, and
   dbt Semantic Layer all live in this space. Palantir Foundry itself
   supports Virtual Objects alongside materialized ones.

Ontosyx's value is the semantic layer and the workbench, not the graph
engine. Forcing migration conflates the product with the storage.

## Decision

**The Virtual Ontology Layer (VOL) is the first-class execution model.**

- The source database(s) are the source of truth for data. Ontosyx never
  requires ETL.
- Queries are compiled to a federated plan over the original sources
  (ADR 0002 picks the engine).
- Ontology concepts (`NodeTypeDef`, `EdgeTypeDef`, ...) are bound to
  physical relations through first-class mappings (ADR 0003).
- A graph database remains available as an *optional cache backend* for
  hot paths that benefit from native graph traversal (ADR 0004).

## Consequences

### Positive

- **Zero sync debt.** There is no CDC, no snapshot freshness SLA, no
  dual-write inconsistency to reason about.
- **Governance inheritance.** Source-level RLS / masking / audit keeps
  applying. Ontosyx adds a workspace predicate on top; it does not
  reimplement tenant isolation.
- **Time-to-value collapses.** Customers connect a DSN and see an
  ontology draft in minutes instead of running a migration project.
- **Platform positioning clarifies.** Ontosyx is a semantic workbench,
  not a graph DB. That is the defensible space.

### Negative

- **Multi-hop performance requires care.** Variable-length paths over
  federated sources are harder than over a native graph DB. Mitigated
  by cost estimation (ADR 0002), optional graph cache (ADR 0004), and
  per-source tier accounting.
- **Some operations are source-dependent.** Recursive CTE availability,
  snapshot syntax, and join pushdown vary. The planner must introspect
  capabilities and either rewrite, route to cache, or reject explicitly
  (`UNSUPPORTED_PATH_ON_SOURCE`).
- **Cross-source write atomicity is impossible.** Actions are
  single-source by contract; cross-source workflows require the Saga
  pattern (Phase 13, separate ADR when implemented).

### Trade-offs made deliberately

- We give up the "one query language, one backend" simplicity of a pure
  graph-DB shop in exchange for the ability to deploy against any
  existing data estate.
- We accept that a subset of graph-theoretic operations will have to be
  bounded or cached; an unbounded `MATCH (a)-[*]-(b)` across a 10-table
  federation is not a use case we promise to optimize.

## Alternatives considered

1. **Mandatory migration to a graph DB.** Rejected — this is the status
   quo and is the single largest reason customers stall adoption.
2. **Graph DB default, VOL as an escape hatch.** Rejected — any
   bifurcation that advantages migration will drift back toward making
   migration the expected path.
3. **GraphQL-style federation with no semantic layer.** Rejected — that
   solves transport but not meaning. The ontology is the product.
4. **Materialize on first query, serve from cache thereafter.** Rejected
   as a default — staleness, cache invalidation, and storage cost
   reintroduce the CDC problem. Retained as an opt-in per-mapping hint
   via ADR 0004.

## Related

- ADR 0002 — federation engine choice.
- ADR 0003 — Mapping as first-class.
- ADR 0004 — Graph DB as optional cache.
- ADR 0009 — Partial-failure policy (implied by federation).
