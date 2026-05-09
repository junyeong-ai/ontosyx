---
status: accepted
date: 2026-05-08
deciders: junyeong-ai
---

# ADR-0036 — Source Contract pre-commit gate

## Context

Mapping commit-paths historically pass an `OntologyIR` through
`validate_with_sources(known_sources)` before snapshotting. That
validator only checks that referenced `source_id`s appear in the
*registered* set — it has no signal for:

- "this `ObjectMappingDef.relation` does not actually exist on
  that source",
- "this `PropertyMappingDef.column` is mapped to a column the
  source never returned",
- "this `LinkMappingDef::Bridge.bridge_relation` is a typo".

All three currently land at runtime as adapter-side failures
(broken queries, silent NULLs, validator rejections in
`SemanticGuardValidator`) instead of at commit time as a typed
validation reject. The commit ships the bad mapping into the
canonical version snapshot, the audit trail records "v3 → v4
landed", and the runtime starts failing on every query that
touches the broken type. Operators rebase / amend-and-republish
to recover; in the meantime evaluations against v4 fail
en-masse.

The audit P0 from the 2026-04-26 deep-dive flagged this gap
explicitly: there is no commit-path enforcement that mappings
match the *physical* source contract, only that they reference
*registered* sources.

## Decision

Promote the source's introspected shape to a typed, persisted
**`SourceContractDef`** — the single source of truth for "what
the source actually returned the last time we asked" — and bake a
pre-commit validator on top.

### Substrate (Φ12.1, this ADR)

`SourceContractDef` lives in `ox-ontology` next to the IR it
constrains:

```rust
pub struct SourceContractDef {
    pub source_id: SourceId,
    pub relation: String,
    pub columns: Vec<ColumnSpec>,        // name + data_type + nullable
    pub primary_key: Vec<String>,
    pub fingerprint: String,             // sha256 over canonical encoding
    pub introspected_at: DateTime<Utc>,
}
```

The fingerprint is canonicalised over sorted columns + sorted
PK so two consecutive introspections of an unchanged relation
produce byte-identical contracts. A fingerprint mismatch on the
inbound row vs the stored row is the schema-drift signal the FE
surfaces as "this source moved on".

`source_contracts` table — workspace-scoped, 4-clause RLS,
`PRIMARY KEY (workspace_id, source_id, relation)`, JSONB
`columns` + `primary_key`, `CHECK` constraints on shape.

`SourceContractStore` trait + Postgres impl exposes:

- `upsert_source_contract` — UPSERT, server-side fingerprint
  recompute (the impl never trusts a client-supplied
  fingerprint; the canonical formula owns it).
- `find_source_contract` — natural-key lookup for the
  introspection pipeline's drift-detection probe.
- `list_source_contracts` — full bank for the commit-path
  validator (single round-trip; the validator walks every
  mapping against the bank).
- `list_source_contracts_for_source` — Source Inspector FE.
- `delete_source_contract` — retraction path.

### Validator (Φ12.2, this ADR)

`OntologyIR::validate_against_source_contracts(&[SourceContractDef])`
walks every `ObjectMappingDef` + `LinkMappingDef` against the
bank and emits diagnostic messages with the same code+params
shape every other IR validator uses.

Diagnostic codes:

- `ontology.validate.object_mapping.relation_not_in_source_contract`
- `ontology.validate.object_mapping.column_not_in_source_contract`
- `ontology.validate.object_mapping.primary_key_column_not_in_source_contract`
- `ontology.validate.link_mapping.endpoint_relation_not_in_source_contract`
- `ontology.validate.link_mapping.endpoint_column_not_in_source_contract`
- `ontology.validate.link_mapping.foreign_key_column_not_in_source_contract`
- `ontology.validate.link_mapping.bridge_relation_not_in_source_contract`
- `ontology.validate.link_mapping.bridge_column_not_in_source_contract`
- `ontology.validate.link_mapping.federated_match_column_not_in_source_contract`

### Soft-skip on first-time setup

The validator returns no diagnostics for a `source_id` that has
no contracts captured *yet*. This is the bootstrap-path
exemption: the operator registers the source, then runs
introspection — and the introspection step is what *populates*
the contract bank. Treating an empty bank as a hard fail would
trap the operator outside the workflow that produces the
contract.

Once introspection has run once for a source, the bank carries
≥1 contract and any subsequent mapping that drifts is caught.
This makes the gate **eventually consistent**: it activates
silently as introspection coverage grows, with no UX cliff.

## Consequences

### Why a separate type from `TableInventoryEntry`

`TableInventoryEntry` carries the *operator-intent axis*: which
tables the project chose to import, which it declined, which
it retracted. It tracks contribution (`contributed_node_ids`,
`contributed_edge_ids`) and a structural digest, but no
per-column data — by design, because the inventory is
workspace-curation metadata and survives schema drift.

`SourceContractDef` carries the *physical-fidelity axis*:
exactly which columns + types + keys the source returned the
last time the kernel asked. It mutates with the source. The
two axes serve orthogonal needs and intentionally do not share
a type — collapsing them would force one of the two to bend
out of shape.

### Φ12.3 commit-path wiring (landed 2026-05-08q)

`complete_ontology_draft` and the canonical-edit commit
(`/api/ontology/edits`) now call
`state.store.list_source_contracts()` and feed the result into
`validate_against_source_contracts`. Violations short-circuit the
commit with the typed `ApiErrorCode::SourceContractMismatch`
(422) — `params.violations` carries the structured diagnostic
list, `params.violation_count` is a flat count for FE summary
copy.

The dry-run pre-check arm of `/api/ontology/edits` (the
`pre_check=true` branch) folds contract violations into the
existing `validation_errors` array so the FE's `would_commit`
flag flips false before the operator hits "commit".

The `ApiErrorCode::SourceContractMismatch` variant lands with
the 4-side parity that `every_variant_has_string_and_class` +
`pnpm error-code-parity-audit` enforce: enum + `as_str` arm +
class (default `ClientError`) + i18n templates in both
`ko.json` and `en.json`. Catalog copy interpolates
`{violation_count}`; the violations list itself is rendered by
the FE error card iterating `params.violations` against the
`errors.ontology.validate.<sub_code>` namespace.

### Φ12.4 capture from introspection (landed 2026-05-08q)

`capture_source_contracts(state, analyzed)` lives on
`helpers/source.rs` and runs after every successful
`analyze_source` invocation:

- `routes/ontology_drafts/lifecycle.rs::create_ontology_draft`
- `routes/ontology_drafts/analysis.rs::reanalyze_ontology_draft`
- `routes/ontology_drafts/extend.rs::extend_ontology_draft`

Per examined `SourceTableDef`, the helper builds a
`SourceContractDef` (column / dtype / nullable / PK) and
upserts via `SourceContractStore::upsert_source_contract`. The
store recomputes the fingerprint server-side so the
canonicalisation rule is the single authority.

`Text` and `CodeRepository` source kinds are skipped silently
(no `schema` to promote — they take a separate path that
doesn't run an adapter introspector).

The capture path is **not** best-effort — a store error
propagates back to the caller as `AppError`, surfacing as the
introspection 5xx so the bank cannot silently fall behind the
ontology draft.

### Φ12.5 column-type compatibility (landed 2026-05-08r)

The validator now also checks that the source column's
data-type categorises into a bucket compatible with the
ontology property's `PropertyType`.

`SourceTypeCategory` is a coarse 11-bucket enum (Boolean,
Integer, Numeric, Text, Date, Timestamp, Duration, Bytes, Json,
Uuid, Unknown). `categorize_data_type(spelling)` is a heuristic
classifier that handles the common dialect spellings —
Postgres / MySQL / BigQuery / Snowflake / DuckDB / SQLite —
collapsing length / precision suffixes and case before matching.
Vendor-specific or unrecognised spellings fall through to
`Unknown`.

The compatibility matrix is intentionally generous:

- `Bool` accepts `Boolean | Integer` (Postgres `bit`,
  `tinyint(1)` are integer-shaped).
- `Int` accepts `Integer` only.
- `Float` accepts `Integer | Numeric` (Int → Float is a
  lossless lift).
- `String` is a catch-all — every category passes (`String` is
  the deliberate untyped escape hatch).
- `Date` accepts `Date | Timestamp` (truncation cast).
- `DateTime` accepts `Timestamp` only.
- `Duration` accepts `Duration` only.
- `Bytes` accepts `Bytes` only.
- `List<…>` and `Map` accept `Json` only.
- `Unknown` is universally compatible — fail-open.

The new diagnostic is
`ontology.validate.object_mapping.column_type_incompatible`
with `params.source_data_type` / `source_category` /
`property_type` / `column` / `property_key` / `mapping_id` /
`source_id` / `relation`.

### Skipped on non-Identity transforms

`PropertyTransform::SqlExpr`, `Concat`, and `Derived` are
operator-authored coercions that intentionally bridge a type
gap — the validator must not flag them as mismatches because
they exist *because* of the type gap. Only
`PropertyTransform::Identity` triggers the type-compat check.

### Skipped when property type can't be resolved

When the parent `NodeTypeDef` can't be found (already flagged
by the topology validator) or the property id doesn't match
any property on the node, the type-compat sub-validator
silently skips — the upstream "unknown_node_type_id" /
"unknown_property_id" diagnostics are the right surface for
those cases; double-reporting noise.

### What's deferred

- **Per-dialect refinement.** The classifier is heuristic by
  design. A future axis could derive the canonical category
  from the source's `source_type` field (Postgres /
  BigQuery / etc.) so a `bit` spelling means Boolean on
  SQL Server but `Integer` on Postgres. The current
  generosity (Bool accepts Integer) sidesteps that today.
- **Type-precision reporting.** The classifier collapses
  `varchar(255)` and `varchar(64)` to the same bucket; an
  operator-facing surface that wants "this column is shorter
  than the property's expected length" needs a separate
  validator on a richer axis.

### Forward-compat notes

- The validator's diagnostic codes follow the existing
  `ontology.validate.*` naming convention so the FE i18n
  catalog absorbs them with no new namespace.
- `compute_fingerprint` is canonicalised — column-order shuffles
  in the introspector's output never produce a drift signal.
- A `SYSTEM_BYPASS` cron sweep that retracts contracts whose
  source has been dropped is straightforward (mirrors
  `verified_query` freshness sweep). Out of scope here.

## Tests

- `source_contract::tests` (substrate, 7 tests) — fingerprint
  stability under reordering, drift-on-content-change,
  PK-driven drift, serde round-trip, case-sensitive lookups,
  `now()`-window timestamping.
- `ir::tests::source_contract_validator` (validator, 8 tests)
  — empty contracts soft-skip, missing relation flagged,
  missing column flagged, source-without-contracts soft-skip,
  PK column flagged, concat-part columns flagged, link
  endpoint relation flagged, bridge relation flagged.

1885 lib + 3 ratchet tests pass.
