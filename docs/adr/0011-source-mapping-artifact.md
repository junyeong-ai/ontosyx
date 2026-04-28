# 0011 — `SourceMappingArtifact` as a first-class IR sibling

**Status:** Accepted

**Date:** 2026-04-29

**Supersedes:** none

## Context

The design action used to inline the source-to-IR translation
inside the LLM prompt. The flow was:

1. Operator clicked "온톨로지 설계".
2. The API called the LLM with the schema + analysis report.
3. The LLM returned an `LlmDesignOutput`.
4. `lifecycle.rs` materialised the output into an `OntologyIR` and
   committed it.

Per-column / per-FK mapping decisions evaporated at step 4: the
final IR carries `ObjectMappingDef` and `LinkMappingDef` (the
"what is mapped where") but nothing about *the decision* — which
prompt produced it, what ambiguities the LLM flagged, what
alternatives were rejected. To answer "did the analysis change
since last week?" the operator had to re-run the design and diff
two IRs, paying the LLM round-trip every time.

Stardog SMS2 and TopBraid R2RML solve this with a declarative
mapping artifact: a versioned, queryable record that names *which
schema snapshot* was being mapped *with which decisions* by *which
prompt at which version*. Re-runs against an unchanged schema
replay the artifact instead of re-prompting.

## Decision

`crates/ox-ontology/src/source_mapping.rs::SourceMappingArtifact`
makes that bridge a first-class IR sibling. Each artifact carries:

- `id: SourceMappingArtifactId`
- `source_id` — the data source the schema lives in
- `schema_snapshot_hash` — SHA-256 of the canonical-JSON
  serialisation of the `SourceSchema` at authoring time. Same
  schema = same hash; a column add / rename / type change produces
  a new hash and a new artifact.
- `property_mappings: Vec<PropertyMappingDef>` — column → property
  bindings, reusing the canonical `PropertyMappingDef` so artifact
  decisions flow verbatim into `OntologyIR.object_mappings`.
- `edge_mappings: Vec<EdgeMapping>` — FK / bridge / computed /
  federated edges (mirrors `LinkMappingKind`).
- `open_questions: Vec<OpenQuestion>` — ambiguities the LLM flagged
  for operator review.
- `provenance: ArtifactProvenance` — `prompt_id`, `prompt_version`,
  `model_id`, free-form `params` map for replay / debug.
- `created_at` / `created_by` — author trail.

The store layer (`SourceMappingArtifactStore` in `ox-store`) is
content-addressed: inserts collapse on
`(workspace_id, source_id, schema_snapshot_hash, content_hash)` so
re-running the design action against an unchanged schema replays
the previous artifact instead of writing duplicates. Naming
follows ADR 0005 — American spelling (`Artifact`), no British
"Artefact" hits in the codebase.

## Consequences

**Positive.**

- Reproducibility: same schema + same prompt = same artifact id.
- Audit: every mapping decision points back to the prompt that
  produced it. Compliance / review questions become queryable.
- Diffing: re-runs surface column-level changes by comparing two
  artifacts' `property_mappings`, not by re-prompting and diffing
  IRs.
- Future: the design review surface can render the artifact's
  `open_questions` as inline operator decisions; a resolved
  question feeds into the next analyse pass as a hint.

**Negative.**

- The design action becomes a two-step write (artifact + IR)
  instead of one. The atomicity is achieved by writing the
  artifact first (no consumers yet) and then committing the IR;
  if the IR commit fails, an orphan artifact remains, which is
  harmless and dedupes on retry.
- Serialising the artifact body twice (once to compute the
  content hash, once to persist) is unavoidable but cheap
  compared to the LLM round-trip the whole flow saves.

## Implementation status

This ADR ships the foundation:

- the artifact type + content-hash helper (`source_mapping.rs`)
- the `SourceMappingArtifactStore` trait + Postgres impl
- the `source_mapping_artifacts` table with full RLS

Wiring the design action to author artifacts on every LLM run is
a follow-up phase. The brain side needs to surface
`prompt_id` / `prompt_version` / `model_id` / `params` on its
output so the lifecycle can stamp each artifact correctly without
re-deriving the provenance after the fact.
