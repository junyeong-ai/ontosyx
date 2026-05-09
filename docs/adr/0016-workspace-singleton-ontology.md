# 0016 — Workspace × Ontology = 1:1 (singleton invariant)

**Status:** Accepted

**Date:** 2026-05-04

**Supersedes:** none — the singleton invariant tightens an earlier
decision (multi-ontology per workspace was permitted but never
required by the product surface) without superseding any prior ADR.

## Context

The original schema treated `workspaces` and `ontologies` as a
many-to-many relationship — a workspace could own zero or many
ontologies; an ontology was scoped by `workspace_id` plus a
free-form `name` / `lineage_id` pair. Two consequences fell out
of that flexibility:

- **No "the ontology" accessor.** Every code path that wanted "the
  workspace's canonical schema" had to either pass an
  `ontology_id` from the URL or pick one from the list — the
  product surface (`/(workbench)/canvas`, `/settings/quality`,
  the chat agent) had to assume "first ontology" or thread an id
  through every component.
- **Cross-source identity broke.** Two ontologies in the same
  workspace could each name a `Customer` NodeType; nothing in
  the schema enforced that they meant the same business entity.
  Federation across mappings depended on string-matching labels
  to recover the lost identity, which drifted under renames.

Foundry, Stardog Knowledge Toolkit, Neo4j Aura — every validated
knowledge-graph platform — pin "one ontology per workspace" as
the canonical model. The KG's value lives in the cross-entity
connections, and per-project ontologies break the "same Customer"
guarantee that connection reach depends on.

## Decision

`ontologies(workspace_id)` carries `UNIQUE`.
The schema rejects a second ontology being created in the same
workspace; the workspace IS the ontology context.

Two structural consequences:

1. **`ontology_drafts.ontology_id` was a redundant FK** back into
   the workspace's only ontology. With singleton enforced, the
   `workspace_id` IS the ontology pointer, so drafts carry the
   workspace and version parent, not a second ontology reference.

2. **The compound uniqueness constraints**
   (`ontologies_ws_id_uq`, `ontologies_ws_name_uq`) become
   redundant — workspace-uniqueness already implies workspace-
   scoped name + id uniqueness. The baseline schema keeps only
   `ontologies_workspace_singleton_uq UNIQUE (workspace_id)`.

The Rust + FE access pattern follows:

- **BE store** — `OntologyVersionStore::get_workspace_ontology()`
  is the canonical accessor for product code. Routes and workers
  should not require an ontology id when the workspace context
  already determines it.
- **API** — every `/api/ontology/*` route is **singular** and
  carries no `{id}` segment. The resolver picks the workspace
  ontology automatically from the request's `WORKSPACE_ID`
  task-local. Id-scoped ontology routes are outside the product
  contract because the workspace already determines the ontology.
- **FE** — `useWorkspaceOntology()` is the singular hook every
  workbench surface reads. There is no ontology selector in
  the UI; the workspace pill in the header doubles as the
  ontology indicator.
- **Korean copy** — the canonical-noun pair settled on
  "대표 온톨로지" (representative / canonical ontology). The
  earlier "캐노니컬" loanword and "커밋된 온톨로지" descriptive
  forms both retire.

## Consequences

- **`OntologyDraft.parent_version_id`** (per ADR-0014's stage 2 +
  this ADR) is a version-axis pointer rather than an
  identity-axis pointer; the workspace × ontology = 1:1
  invariant already pins the identity, so the only piece worth
  recording per draft is which exact `ontology_version_snapshots`
  row it branched from.
- **Cross-source merge is straightforward.** Two source mappings
  (CRM `customer_id` + ERP `buyer_id`) now bind to the same
  `Customer` NodeType inside the singleton ontology by
  construction. Federation walks the shared NodeType id
  without resolving through label heuristics.
- **No "select your ontology" UX.** The product surface saves a
  full screen of selection chrome on every workbench page.
- **Multi-tenancy axis is workspace-only.** A user with access
  to two workspaces sees two ontologies; sharing is at the
  workspace level (workspace members), never at the ontology
  level. Access control collapses to the existing workspace
  ACL surface.

## Alternatives considered

- **Keep many-to-many, add a "default ontology" pointer on
  `workspaces`** — rejected. The pointer becomes another field
  to migrate / lock / surface; it doesn't structurally enforce
  the invariant; cross-ontology identity stays broken.
- **Multi-ontology per workspace with cross-ontology mapping**
  — rejected. Validated platforms with this shape (custom OWL
  imports, federated SPARQL endpoints) all sink time into
  reconciliation tooling that the singleton invariant makes
  unnecessary. The cost-benefit didn't justify the build.
- **Per-user ontology scoping** — rejected. A workspace's
  members share the schema by design; per-user shapes are a
  collaboration anti-pattern.

## References

- Memory entry: `feedback_workspace_singleton_ontology.md`
- Memory entry: `feedback_canonical_korean_term.md`
- Schema `crates/ox-store/migrations/0001_schema.sql`
- Foundry Ontology docs (Palantir public)
- Stardog Knowledge Toolkit (`stardog.com/docs/`)
