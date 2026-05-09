# ox-api

Axum HTTP server. Binary name: `ontosyx`.

## Ontology-draft create flow — `selection` is required and explicit

`POST /api/ontology-drafts` and the sibling `extend` / `reanalyze` endpoints take `selection: AnalyzeSelection` as a **required** field (`{"kind": "all"}` for a full sweep, `{"kind": "subset", "tables": [...]}` for a curated list, `{"kind": "extend", "tables": [...]}` to grow an existing baseline). There is no implicit full-warehouse default — designers pick deliberately so usage-billed backends (BigQuery, Snowflake) only pay introspection cost on the chosen tables.

Empty `subset` / `extend` lists are rejected by `AnalyzeSelection::validate()` at the request boundary; subset names that don't appear in `list_tables()` produce a `Validation` error naming the missing tables (no silent drop).

Frontends drive a two-phase flow: `POST /api/ontology-drafts/source-preview` returns the cheap dataset listing, the user curates with `<TableSelector>`, then `POST /api/ontology-drafts` is called with the explicit subset.

## DTO naming

Action DTOs follow `Verb + Noun + (Request|Response)` — e.g. `CreateProjectRequest`, `UpdateOntologyDraftDecisionsRequest`, `PreviewSourceRequest`, `ExtendOntologyDraftResponse`. Read-only data shapes that carry no verb (single-resource responses, error envelopes, summary blocks) keep their noun-only names — `WorkspaceResponse`, `ErrorResponse`, `AdapterAnalysisResponse`, `FederationHealthResponse`.

## Design gates — single evaluator, single source of truth

The design action is the only LLM-driven entry point that consumes operator review state (column clarifications, partial-analysis acknowledgement, large-schema acknowledgement). Whether it may proceed is decided by a single evaluator, `ox_ontology::design_gate::evaluate_design_gates`. The same vector serves three callers:

- **Backend enforcement** — `design_ontology_draft` and `design_ontology_draft_stream` call `enforce_design_gates(report, options)` and reject with a structured 422 (`code: "design_gates_unmet"`) when any blocking gate is unmet. Other ontology actions don't gate: `extend` runs analyze+design as a single call so there is no review checkpoint, `reanalyze` doesn't invoke the LLM, and `refine` / `edit` operate on the already-validated ontology rather than the analysis report.
- **API response** — `OntologyDraftView` (the wrapper every project-returning endpoint emits) carries `design_gates: Vec<DesignGate>` so the FE renders the checklist without re-deriving the rules.
- **FE checklist** — `DesignGateChecklist` renders one row per gate with i18n copy keyed by `gate.id` and parameters interpolated from `gate.params`. Click on an unmet gate scrolls to `gate.anchor`. Adding a new gate = one `GateId` variant + one match arm in `evaluate_design_gates` + one i18n key.

## Language-neutral wire shape

Backend never produces user-facing prose for infrastructure messages. `AnalysisWarning`, `DesignGate`, error responses carry `class` / `id` enums plus an interpolation-args map; the FE i18n catalogue owns the locale. Content (DB-persisted ontology descriptions, glossary terms, rule rationale) uses `LocalizedText` with explicit translations — that's user-authored data, not platform copy.

## Adding a New Route

1. Create handler in `routes/my_feature.rs`.
2. Add `pub mod my_feature;` in `routes/mod.rs`.
3. Register route in the `protected` router in `routes/mod.rs`.
4. Role enforcement at handler top: `principal.require_admin()?` or `principal.require_designer()?`.
5. Workspace-scoped data: add `ws: WorkspaceContext` parameter and use `ws.workspace_id` for new records. RLS enforces isolation on reads automatically.

## Middleware Stack (order matters)

`require_auth` → `workspace_context` → `audit_log` → handler.

## Workspace Context in Async Tasks

`tokio::spawn` does NOT carry workspace context (task-locals are lost). Use `crate::spawn_scoped::spawn_scoped` instead — it captures `WORKSPACE_ID` and `GRAPH_WORKSPACE_ID` into the spawned future.

## Public Endpoints (no auth)

Public routes (e.g., shared dashboards) bypass auth but RLS still blocks queries. Wrap store calls with `ox_store::SYSTEM_BYPASS.scope(true, async { ... })`. Always filter response fields to exclude internal data (workspace_id, user_id, etc.).

## Model Management

- `DbModelRouter` implements `ModelResolver`, reads from DB with 30s TTL cache.
- After any model config change, call `state.model_router.invalidate().await` and `state.client_pool.invalidate_all()`.

## Chat Streaming

- `POST /api/chat/stream` returns SSE events.
- `model_override` field in request → `RunConfig` for per-request model switch.
- Agent uses branchforge's `execute_stream_with()`.

## MCP Server

`mcp.rs` exposes ontology tools via the `rmcp` crate (separate from branchforge's MCP client). Custom domain logic — not a candidate for branchforge delegation.

## Collaboration WebSocket protocol

`/ws/collab` is the only non-HTTP route. It carries the realtime collaboration channel — presence, cursor sharing, entity locks. The wire types live in `crate::collaboration` (`ClientMessage`, `ServerMessage`, `ErrorCode`, `PresenceInfo`, `CursorPosition`) and are emitted into the OpenAPI `components/schemas` block — generated FE clients read them via `components["schemas"]["ClientMessage"]` etc.

Direction is split at the type level: `ClientMessage` is what the client sends, `ServerMessage` is what the server returns. Mixing the two would let either side accidentally produce the other's frames.

Auth flow:
1. Client opens `/ws/collab`.
2. First frame within `AUTH_TIMEOUT` (5s) MUST be `ClientMessage::Authenticate { token, workspace_id }`.
3. Server validates the JWT, confirms membership in `workspace_id` via RLS-backed `get_workspace`, reserves a session slot (`max_sessions_per_user`), and replies with `ServerMessage::Authenticated`.
4. The remainder of the connection runs inside `WORKSPACE_ID` + `GRAPH_WORKSPACE_ID` task-locals — every store/graph call rejects cross-workspace identifiers automatically. `Join { ontology_draft_id }` calls `get_ontology_draft` and a foreign id resolves to `None` → `ErrorCode::UnauthorizedOntologyDraft`.

Entity locks have a TTL (`collaboration.lock_ttl_secs`, default 300s). Stale locks are reaped on contention; `LockGranted.expires_at` carries the deadline so clients can renew. `LockGranted` broadcasts to the room (clients filter their own request via `entity_id`); `LockDenied` is unicast to the requester only.

Cursor events are throttled at the hub (`collaboration.cursor_throttle_ms`, default 50ms per user-room pair). Floods inside the window are silently dropped.

Adding a new collaboration message: extend `ClientMessage` or `ServerMessage`, add the matching `Hub` method, wire the variant through `ws::serve_collab`'s match block. The OpenAPI spec picks up the schema change automatically — no separate registry edit.

## Typed error model — `ApiErrorCode` + `params`

Every error response (HTTP body + SSE `error` event) carries the
typed wire shape:

```json
{ "error": { "code": "not_found", "class": "client_error", "params": { "entity": "OntologyDraft" } } }
```

- `code` — `ApiErrorCode` enum (snake_case wire string, see
  `error.rs::ApiErrorCode::as_str`).
- `class` — `client_error` (4xx) or `server_error` (5xx).
- `params` — interpolation values for the FE i18n catalog at
  `errors.<code>`. **The backend never produces user-facing prose**
  — that's the FE's locale concern.

**Don't** use `AppError::bad_request(format!("..."))` for anything
user-facing. The English string lands in `params.detail`, which the
i18n template interpolates verbatim — Korean users see English.
The right move is a domain-specific typed code:

1. Add an `ApiErrorCode` variant (`OntologyVersionConflict`).
2. Mirror it in `as_str()` and the
   `every_variant_has_string_and_class` test array.
3. Add a typed constructor that takes structured params
   (`AppError::ontology_version_conflict(expected, current)`),
   not free-form strings.
4. Add `errors.<code>` templates to `web/messages/{ko,en}.json`.

`pnpm error-code-parity-audit` (CI gate) parses `as_str` and
asserts every wire string has a matching template in both
bundles. No silent drift.

5xx params stay empty by convention — driver text never reaches
the wire body. Operators correlate via the `x-request-id`
response header set by the request-id middleware. The
`runtime_5xx_redacts_driver_text` test pins this.

## Cron tasks — `CronTask::singleton_key()` for shared writes

Background sweeps that mutate shared state (stale-concept,
quality-baseline, soft-delete compaction, draft-checkpoint
cleanup) override `CronTask::singleton_key()` to return
`Some(ADVISORY_LOCK_CRON_<NAME>)`. The scheduler then wraps each
tick in `pg_try_advisory_lock`; only the holding replica runs the
sweep, others skip silently.

In-process-state tasks (clarification evict, collaboration idle
reap) keep `singleton_key()` as `None` (default) — every replica
runs on its own memory.

`spawn_cron(task, Some(pool), cancel)` is the production call;
`spawn_cron(task, None, cancel)` is test-only.

## Notification dispatch — `EventPayload` trait + `dispatch_event<P>`

Every webhook-emitted event is a value type implementing
[`EventPayload`] (`crate::notifications`):

```rust
pub(crate) trait EventPayload {
    fn event_type(&self) -> NotificationEventType;
    fn subject(&self) -> String;
    fn render(&self, channel: &NotificationChannel) -> serde_json::Value;
}
```

The single generic `dispatch_event<P>(store: &dyn NotificationStore, ws_id, payload)`
fans out — list channels subscribed to `payload.event_type()`,
render once per channel, POST through the shared
`WEBHOOK_CLIENT`, persist a `NotificationLog` row whether the
delivery succeeded or failed. Adding a new event = a new
payload struct + a 3-method `EventPayload` impl + a thin
public dispatcher wrapper. The fan-out machinery does not
change.

`render(channel: &NotificationChannel)` (not `channel_type`)
so payloads can weave channel metadata (`channel.name`) into
the generic envelope — every Generic-webhook payload carries
`channel_name` so a downstream listener that fans in multiple
channels can attribute each delivery.

The dispatcher takes `&dyn NotificationStore` (narrow), not
`&dyn Store` — a focused contract that keeps the dependency
explicit and lets `StubNotificationStore` in `test_support`
drive integration tests without standing up Postgres.

Slack section bodies are clamped through `clamp_slack_text`
(2900-byte budget reserved against `'…'.len_utf8()`) so
large-cardinality alarms (100+ alert cells) never overflow
Slack's 3000-char limit.

The SSRF guard on `validate_webhook_url` parses through
`url::Host` + `IpAddr` — never prefix-string heuristics —
matching RFC1918 / loopback / link-local / IPv6 ULA
(`fc00::/7`) exactly, and a small explicit denylist for
non-DNS-resolvable hostnames (`localhost`,
`host.docker.internal`, `kubernetes.default[.svc]`).

`NotificationEventType` (subscribable) is a strict subset of
`NotificationLogEventType` (log row tag). The total
`from_subscription` const fn promotes a subscription event to
its log mirror; the parity test
`notification_log_event_type_mirrors_every_subscription_event`
fails if a future subscription variant lacks a log mirror.
