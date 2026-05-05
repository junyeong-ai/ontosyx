# 0023 — `HeuristicProposal` queue + "no automatic decisions" invariant

**Status:** Accepted

**Date:** 2026-05-01

**Supersedes:** none

## Context

LLM-driven design surfaces (the design pipeline, glossary
suggestion, mapping inference, ambiguity resolution) generate
many candidate decisions per session. Three failure modes are
common across the industry:

- **Auto-apply.** The LLM proposes "merge `customer_v2` into
  `customer`" and the system applies it without operator review.
  A wrong merge is hard to undo, especially after downstream
  joins fan out.
- **Lossy queue.** Proposals are emitted as toast notifications
  the operator dismisses; the system has no record of "what was
  considered, what was rejected, why".
- **Confidence drift.** The same suggestion arrives every
  session because the system doesn't remember the operator
  rejected it last time.

Foundry Logic, Stardog Voicebox, dbt Semantic Layer all enforce
the same invariant: **never apply an LLM-driven schema decision
without an explicit operator approval, and keep the proposal
record durably so the next session can dedupe**.

## Decision

`HeuristicProposal` becomes the typed transport for every LLM-
or rules-derived suggestion that would otherwise need to mutate
shared state. Four invariants the system enforces:

1. **No automatic decisions** — every code path that would
   commit a schema mutation from an inferred source must route
   through a `HeuristicProposal` row. Direct writes from
   inference helpers are a code-review-blocking anti-pattern.

2. **Proposal queue is durable** — proposals land in a
   workspace-scoped store table (RLS-protected). The operator
   surface walks the queue; rejected proposals stay in the
   queue with `status = rejected` so the same suggestion does
   not bubble up next session.

3. **Threshold gating** — `confidence_bps` (basis points,
   `u16`) measures the inference engine's confidence in the
   proposal. Proposals below the workspace threshold drop
   silently into a "low confidence" sub-queue the operator
   can opt into; above threshold surface in the main queue.

4. **Audit trail** — every proposal carries
   `prov:wasGeneratedBy` (per ADR-0008) naming the activity
   (`prompt_template + model + render_hash` for LLM proposals,
   `rule_id + applied_at` for rule-derived). Approval emits a
   matching `prov:Activity` so "who decided what, when, on
   whose suggestion" is queryable end-to-end.

## ConfidenceBps

`ConfidenceBps(u16)` is the canonical confidence unit:

- 0 — useless (never propose)
- 5000 — middling (50%)
- 9500 — strong (95%, default workspace threshold)
- 10000 — certainty equivalent (rule-derived, deterministic)

The basis-points unit avoids float-comparison pitfalls in
threshold gating and serialises to a stable integer wire shape.
Future per-source / per-axis thresholds reuse the same unit.

## Consequences

- **Inference engines stay testable** — a proposal-emitting
  function returns `Vec<HeuristicProposal>`, not side-effects.
  Tests assert "the engine proposes X with confidence Y" without
  a database.
- **Operator UX is decoupled from inference timing** — the
  queue surface is a Settings page, not a modal mid-session.
  Operators can review proposals on their own cadence.
- **Multi-replica safety** — the queue is the only durable
  write surface; concurrent inference runs from different
  replicas append to it (UPSERT on the natural key
  `(workspace, source_kind, target_id, signature)`) instead
  of racing on the schema mutation.
- **LLM cost contained** — once a proposal is rejected, the
  proposal generator dedupes by signature on the next
  invocation; the LLM does not re-pay tokens to repropose
  the same merge.

## Open follow-ups

The trait + queue table are committed; the inference engines
that should route through it are still in transition. The
following grep, run periodically, surfaces direct-write code
paths that should migrate:

```
rg "OntologyCommand::(Add|Delete|Rename).*(suggest|infer|hint|llm)"
```

Each hit is either a legitimate operator-driven write or a
candidate for `HeuristicProposal` routing.

## Alternatives considered

- **Free-text suggestion log** — rejected. Without typed
  proposals, dedup-by-signature is impossible; the same
  suggestion bubbles up indefinitely.
- **Auto-apply above 99% confidence** — rejected. Confidence
  scores from LLMs are not calibrated; the operator-in-the-
  loop guarantee is the platform's safety story for shared
  schema mutations.
- **One queue per inference axis (mapping, glossary,
  ambiguity)** — rejected. Operators triage on the same
  Settings page; partitioning by axis fragments the surface
  without simplifying the queue logic.

## References

- W3C PROV-O — <https://www.w3.org/TR/prov-o/>
- ADR-0008 — PROV-O lineage
- Memory entry: `feedback_advisory_lock_pattern.md`
  (parallel: durable shared-write surfaces always go through
  a typed coordination layer, never inline state mutation)
