# ADR 0010: Canonical IRI scheme for ontology entities

- Status: Accepted
- Date: 2026-04-20

## Context

Every first-class entity in `ox-ontology` has a UUID (`XxxId`). UUIDs
are stable but not discoverable: they cannot be dereferenced, they
carry no semantic hint, and they cannot be embedded in a Turtle / OWL
export without an accompanying namespace declaration.

For a semantic platform, an IRI is a first-class identifier. It lets
external tooling:

- dereference an entity (if we later host a content-negotiated
  endpoint),
- reference an entity from foreign RDF / OWL documents,
- import / export through Turtle, JSON-LD, SHACL, PROV-O without
  ad-hoc mapping,
- appear in data-catalog, SIEM, and compliance systems that already
  understand URIs.

The scheme has to be chosen deliberately because it becomes part of
exported artifacts and is effectively immutable once customers
reference it.

## Decision

Every first-class ontology entity has a **canonical IRI** alongside its
`XxxId`:

```
https://ontosyx.io/onto/<workspace_slug>/<kind>/<entity_slug>
```

Rules:

- `workspace_slug` — lowercase, kebab-case, unique per tenant.
  Generated from the workspace display name; unique constraint in
  `ox-store`.
- `<kind>` — one of:
  `node-type`, `edge-type`, `interface`, `property`, `rule`,
  `function`, `action`, `metric`, `enrichment`, `glossary-term`,
  `object-mapping`, `link-mapping`, `data-quality`, `provenance`,
  `audit-event`, `schema-drift`.
  The literal strings are part of the contract.
- `entity_slug` — lowercase, kebab-case, unique within
  `(workspace, kind)`. Generated from the entity label with a UUID
  suffix fallback on collision.
- Scheme is fixed to `https://` even on private deployments;
  private deployments override `ontosyx.io` with their own
  `<tenant_domain>` via workspace configuration.
- Trailing slash is never significant; the canonical form omits it.
- Versioning is **not** in the IRI. An entity keeps the same IRI
  across ontology versions; the version is carried by
  `ProvenanceDef.ontology_valid_at` (ADR 0007, 0008). This matches
  `owl:versionIRI` convention: the term IRI is stable, the version
  IRI is separate metadata.

All JSON serializations that cross a user boundary (API responses,
exports, audit records) include both `id` (UUID) and `iri` (URL).

## Consequences

### Positive

- Turtle / JSON-LD / SHACL / PROV-O export is mechanical — the
  subject of every triple is already an IRI, not a synthesised one.
- Cross-workspace references are expressible without UUID collision
  (the `workspace_slug` segment disambiguates).
- Human-readable IDs in logs and URLs. Support investigating
  "which node type did the customer mean?" becomes easier.
- Stable identity across renames: the IRI's `entity_slug` is derived
  once and never changes, even when the display label changes.

### Negative

- Slugs can collide. Mitigation: unique constraint + numeric suffix
  (`-2`, `-3`) + `AuditEventDef` entry on collision resolution.
- Labels with CJK / non-Latin characters need transliteration for
  the slug. We use the original `label` in `display_name` and derive
  `entity_slug` from the ASCII `name` the user (or LLM) provided.
  When neither exists, we fall back to the last 12 hex of the UUID.
- Private deployments wanting a custom authority must set the
  workspace's `iri_authority` at creation. Changing it later
  rewrites every exported IRI; supported but audited.

### Trade-offs

- We accept a global convention in exchange for interoperability.
- We resist including version in the IRI despite the temptation;
  versioned IRIs cause `owl:sameAs` proliferation in practice.

## Alternatives considered

1. **UUID-only, synthesize IRIs at export time.** Rejected — exports
   become bespoke, and foreign references (customer A's ontology
   citing customer B's enrichment) are impossible.
2. **Versioned IRIs (`.../node-type/customer/v5`).** Rejected — ontology
   time is a separate axis (ADR 0007); encoding it in the IRI creates
   five problems for every one it solves.
3. **`urn:ontosyx:...`.** Considered; rejected because HTTPS IRIs are
   dereferenceable and align with the rest of the RDF tooling
   ecosystem.

## Related

- ADR 0005 — naming; every entity has `pub iri: Iri`.
- ADR 0006 — SHACL shape IRIs are issued under this scheme.
- ADR 0008 — PROV-O subjects are these IRIs.
