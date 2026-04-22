# ADR 0009: Partial-failure policy for federated execution

- Status: Accepted
- Date: 2026-04-20

## Context

ADR 0001 + 0002 make federation the default execution model. Federation
introduces a category of failure that single-source databases do not
have: **one source fails, others succeed**.

Options historically taken by federated systems:

- **GraphQL federation** returns partial data with an `errors[]` array.
- **Trino** fails the entire query if any worker fails.
- **Dremio** and **Denodo** expose per-source fallback / timeout flags.
- **Salesforce SOQL cross-org federation** serves a downgraded response
  from cache when the remote silver is unavailable.

Each choice is defensible; the mistake is choosing implicitly. A
federation engine without a partial-failure contract will behave
differently on every code path, and a customer will discover the
inconsistency only after it hurts.

## Decision

Every query carries an explicit `partial_failure: PartialFailureKind`.

```rust
pub enum PartialFailureKind {
    /// Default. Any source failure aborts the query.
    FailFast,
    /// Return successful per-source results; failures listed in
    /// response.errors[] with source id + error class + retryable flag.
    AllowPartial,
    /// If a required source is unavailable AND a graph-cache mapping
    /// exists AND the cache is within the declared staleness budget,
    /// serve from cache with an explicit degraded marker.
    DegradedFromCache { max_staleness: Duration },
}
```

Response envelope (both REST and streaming):

```jsonc
{
  "data": { /* result columns */ },
  "errors": [
    {
      "source_id": "...",
      "class": "transient | permanent | timeout | authz | unsupported",
      "message": "...",
      "retry_after_ms": 5000
    }
  ],
  "degraded": {
    "from_cache": true,
    "staleness_ms": 42000,
    "mapping_ids": ["..."]
  }
}
```

Semantics:

- `FailFast` is the default. Raising the contract requires an explicit
  request, a log entry, and (for `DegradedFromCache`) per-workspace
  governance allowing degraded service.
- `AllowPartial` never drops rows silently. Every missing source is
  enumerated, and client code may render partial results with a
  warning banner.
- `DegradedFromCache` is the only path where the graph cache is used
  without an explicit opt-in per mapping. It still honours
  `max_staleness`; beyond that the request fails.
- Error classification is stable and part of the API contract:
  `transient | permanent | timeout | authz | unsupported`.

## Consequences

### Positive

- Failure behaviour is part of the query, not a hidden server flag.
- Observability is clean: `errors[]` is present even when the response
  is a 200, and is empty when the data is fully trustworthy.
- Dashboards and LLM agents can make informed decisions ("this
  widget's revenue figure is from cache, 2 minutes stale").
- Security reviews have a single place to verify that failure modes
  cannot leak data they shouldn't.

### Negative

- API clients must handle `errors[]`. Existing clients assuming
  "status 200 means clean data" will need updating — acceptable
  because there are no public integrations at v3.
- `DegradedFromCache` requires cache existence + freshness + policy,
  making it a heavier feature than a raw flag. We keep it opt-in at
  the workspace-governance level.

### Trade-offs

- We pay slightly heavier response envelopes (always two extra
  fields) for the ability to reason about trust.

## Alternatives considered

1. **Fail-fast only.** Rejected — does not serve dashboard / agent use
   cases where partial answers still help.
2. **Implicit partial with `errors[]`.** Rejected — implicit partial
   behaviour is the source of long-tail bugs in every federated
   system we surveyed.
3. **Per-source timeout / retry flags.** Retained as a lower-level
   tunable (`QueryBudget`), but the top-level failure contract is the
   enum above, not a soup of booleans.

## Related

- ADR 0001 — VOL + federation make this necessary.
- ADR 0004 — `DegradedFromCache` is the controlled path to the graph
  cache on source failure.
- ADR 0008 — Provenance records the degraded flag so later audits see
  which results were partial.
