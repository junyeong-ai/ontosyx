# 0017 — Typed error wire shape (`{ code, class, params }`)

**Status:** Accepted

**Date:** 2026-05-04

**Supersedes:** ad-hoc `bad_request("English string")` calls
in routes; the new shape is the only path to surface an error
to the operator.

## Context

The original error pattern routed every API failure through
`AppError::bad_request(format!("..."))` with English prose.
Three failure modes followed:

- **English leaks into Korean UI.** The FE rendered the prose
  verbatim — a Korean operator saw `"Project status mismatch:
  required 'designed', got 'analyzed'"`.
- **No FE typing.** The FE could only switch on the HTTP status
  code (400 / 404 / 409 …), so an "ontology version conflict" and
  a "draft has no ontology" both rendered the same generic 400
  toast.
- **No catalog discipline.** Adding a new error message meant
  editing the route's prose; the i18n catalogue had no canonical
  list of what could go wrong.

Industry-validated error envelopes (RFC 7807 Problem Details,
Stripe error.code, Twilio code+more_info) all converged on the
same idea: a stable typed `code` plus structured `params` the
client renders against its own copy.

## Decision

Every API error response (HTTP body + SSE `error` event) carries:

```json
{
  "error": {
    "code": "ontology_draft_stale_parent",
    "class": "client_error",
    "params": { "parent_version": "v3", "current_version": "v5" }
  }
}
```

- **`code`** — `ApiErrorCode` enum (snake_case wire string,
  see `error.rs::ApiErrorCode::as_str`). Stable identity. The
  test `every_variant_has_string_and_class` pins
  the enum-to-string mapping at compile time.
- **`class`** — `client_error` (4xx) or `server_error` (5xx).
  The FE branches on this for "is this my mistake or
  yours" rendering.
- **`params`** — interpolation values for the FE i18n catalog
  at `errors.<code>`. The backend never produces user-facing
  prose. `params` for 5xx responses stay empty by convention;
  driver text never reaches the wire body. Operators correlate
  via the `x-request-id` response header set by the request-id
  middleware.

Adding a new code is a four-side sync:

1. Add an `ApiErrorCode` variant in `crates/ox-api/src/error.rs`.
2. Mirror it in `as_str()` and the
   `every_variant_has_string_and_class` test array.
3. Add a typed constructor that takes structured params
   (`AppError::ontology_version_conflict(expected, current)`),
   not free-form strings.
4. Add `errors.<code>` templates to `web/messages/{ko,en}.json`.

`pnpm error-code-parity-audit` (CI gate) parses `as_str` and
asserts every wire string has a matching template in both
bundles. No silent drift.

The FE renders prose by reading the i18n catalogue:

```ts
toast.error(error.localize(t)); // t = useTranslations("errors")
```

`error.message` is never read by the FE; `params.detail` is
never interpolated into raw English — the catalogue template
owns the locale.

## Phased migration

Existing `AppError::bad_request("...")` call-sites migrate one
domain at a time. Each migration is the same four-step
playbook:

1. Add the typed `ApiErrorCode` variant for the domain (e.g.
   `OntologyDraftStaleParent`, `OntologyDraftMissingSourceSchema`,
   `OntologyDraftStatusMismatch`).
2. Add the typed constructor that accepts structured params.
3. Replace every `bad_request(format!("..."))` in the domain
   with the typed constructor.
4. Add the `errors.<code>` templates to both bundles.

The audit catches drift; the migration is incremental and
reviewable per domain. Domains that have not yet migrated keep
their `bad_request` calls — the typed-vs-prose split surfaces in
the catalogue parity check, never silently.

## Consequences

- **Korean copy is operator-grade.** Every error renders as
  catalogue Korean prose with `params` interpolated in the
  natural sentence position (`"v{parent_version} 에서 분기"`,
  not `"branched from v{parent_version}"`).
- **FE branches on typed codes.** `OntologyDraftStaleParent`
  triggers a "rebase before retry" inline action that
  `OntologyVersionConflict` doesn't; the FE knows the
  difference because the `code` is stable.
- **Catalog is the source of truth.** Adding a new error code
  without the catalogue entries fails CI; removing a code from
  the enum without removing the templates also fails. Both
  axes of drift are mechanically caught.
- **Deferred adopters tolerate the mix.** Domains that are
  still on `bad_request` fall through to the catalogue's
  `bad_request` template (which interpolates `params.detail`);
  the migration can continue without an all-or-nothing
  ship.

## Alternatives considered

- **HTTP status only (no `code`)** — rejected. Stripe-style
  matrix of (status, code) is industry standard for a reason;
  status alone is too coarse to drive UX branching.
- **Free-form `kind: String`** — rejected. Loses the test that
  verifies "every code has a template"; loses the enum
  exhaustiveness check that surfaces missing match arms at
  compile time.
- **Server-rendered Korean / English prose** — rejected. The
  FE locale chain is the operator's preference; the BE has
  no clean access to it (per-request locale headers are the
  wrong axis, and operators switch locales via the FE settings
  page mid-session).

## References

- Memory entry: `feedback_typed_error_wire_shape.md`
- Memory entry: `feedback_typed_error_phased_migration.md`
- RFC 7807 Problem Details for HTTP APIs
- Stripe Error Code reference
- `crates/ox-api/src/error.rs::ApiErrorCode`
- `web/messages/{ko,en}.json` `errors.*` namespace
- CI gate: `pnpm error-code-parity-audit`
