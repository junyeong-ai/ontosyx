# Architecture Decision Records (ADR)

Ontosyx uses MADR-lite ADRs to record decisions whose cost of reversal is high
enough to warrant explicit context. Each ADR states the problem, the chosen
path, what we gave up, and the alternatives considered.

A decision lives in an ADR once, in the commit history forever. If a later
decision supersedes an earlier one, a new ADR is added and the older file gets
a `Superseded-By:` header; the original text is preserved so readers can see
why the earlier shape existed.

## Index

| #    | Title                                               | Status   |
|------|-----------------------------------------------------|----------|
| 0001 | Virtual Ontology Layer as first-class execution     | Accepted |
| 0002 | Apache DataFusion as federation engine              | Accepted |
| 0003 | Mapping (Object/Link/Property) as first-class       | Accepted |
| 0004 | Graph database as optional cache backend            | Accepted |
| 0005 | Final naming convention                             | Accepted |
| 0006 | SHACL Core as the rule model                        | Accepted |
| 0007 | Bitemporal semantics (ontology-time + data-time)    | Accepted |
| 0008 | W3C PROV-O aligned provenance                       | Accepted |
| 0009 | Partial-failure policy for federated execution      | Accepted |
| 0010 | Canonical IRI scheme for ontology entities          | Accepted |
| 0011 | `SourceMappingArtifact` as the declarative bridge   | Accepted |
| 0012 | RLS enforcement contract                            | Accepted |
| 0013 | SHACL `sh:message` as diagnostic source of truth    | Accepted |
| 0014 | `ConceptDef` as canonical identity above NodeType   | Accepted |
| 0015 | `SegmentDef` as first-class IR collection           | Accepted |
| 0023 | `HeuristicProposal` queue + no-auto-decisions       | Accepted |

Companion architecture documents live in `../architecture/`:

- `6-axes.md` — the six semantic axes that define the platform identity.
- `crate-dag.md` — final Rust crate dependency graph and responsibilities.

## Writing an ADR

- Number monotonically (`NNNN-kebab-title.md`).
- Keep the body decision-focused: context → decision → consequences → alternatives.
- Prefer concrete over hedging. If the decision needs to be revisited, a new
  ADR supersedes, it does not edit the old one.
