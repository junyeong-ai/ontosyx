# 0013 — SHACL `sh:message` as the rule's diagnostic source of truth

**Status:** Accepted

**Date:** 2026-04-29

**Supersedes:** none

## Context

SHACL violations on a Cypher run flow through
`crates/ox-runtime/src/cypher/shacl_validator.rs::build_issue` →
`ValidationIssue` → `QueryDiagnostic` (wire-shape `code` +
English `message` + `params`). The FE renders that diagnostic by
picking the `next-intl` catalogue template keyed off the
diagnostic `code`. Authors of a `RuleDef` had no way to say
"when this rule fires, render *this* phrase" — the catalogue
template was the only template, and per-violation copy lived in
the rule's `description` / `rationale` fields which the
violation path never consulted.

W3C SHACL Core defines `sh:message` for exactly this case: the
rule author's preferred violation rendering. Stardog and TopBraid
both surface it on every violation; their absence in our pipeline
forced authors to file an i18n catalogue PR for every rule that
needed bespoke wording — slow, lossy, and impossible for
operator-authored rules.

## Decision

`RuleDef.sh_message: Option<LocalizedText>`. Rule-level (not
per-constraint) by design:

- A rule with multiple constraints reads as a single actionable
  unit at the operator's grain — "this thing is wrong" — so the
  message belongs at the rule.
- `ShaclConstraint` is a 15-variant enum; per-variant
  `sh_message` would inflate every variant with the same
  optional field and break source-compatibility every time a new
  variant lands.
- TopBraid and Stardog both store the message at shape-level
  (the SHACL idiom maps cleanly onto our rule).

Routing on emit: `build_issue` injects the author's
`sh_message_<lang>` keys into the diagnostic's `params` for every
locale present on the `LocalizedText`. The wire shape stays
additive — consumers without the new field render unchanged.

Routing on resolution: `useDiagnosticResolver()` (FE) checks for
`sh_message_<lang>` against the workspace locale chain and falls
back to `sh_message_default` before consulting the catalogue. A
rule with `sh_message` set wins; a rule without it routes through
the existing `code` → catalogue template path identically to
before.

## Consequences

**Positive.**

- Rule authors can land bespoke violation copy without touching
  the i18n catalogue.
- Bilingual deployments stay single-source — one rule, one
  `LocalizedText`, every locale picks its translation off the
  same record.
- Backwards-compatible: rules without `sh_message` continue to
  render via the catalogue template.

**Negative.**

- Two paths exist for "the user-facing violation phrase" —
  catalogue template vs `sh_message`. The resolver picks
  `sh_message` first to make the intent unambiguous, but
  reviewers need to remember the precedence when editing a rule
  whose copy looks "stuck" in old wording.
- `sh_message_<lang>` params are reserved namespace on the wire
  — a future diagnostic param named `sh_message_*` would
  collide. Mitigation: the prefix is descriptive enough that no
  unrelated diagnostic param needs it; the resolver doesn't
  treat the params as opaque.

## Out of scope

`sh_message` on `ConstraintDef` (NodeType-level structural
constraints). Those have their own catalogue path
(`ontology.validate.constraint.*`) and are not currently a
violation surface — they fail at apply time, not at runtime
query.
