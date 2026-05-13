# ox-api

Axum HTTP server. Binary name: `ontosyx`.

## Middleware stack (order matters)

`require_auth` → `workspace_context` → `audit_log` → handler.

- Workspace-scoped data: add `ws: WorkspaceContext` to the handler signature and use `ws.workspace_id` for new records. RLS handles read isolation automatically.
- Public routes (shared dashboards) bypass auth but RLS still blocks queries — wrap store calls with `ox_store::SYSTEM_BYPASS.scope(true, async { ... })` and filter response fields to exclude internal data (`workspace_id`, `user_id`, …).

## Adding a route

1. Handler in `routes/<feature>.rs`; `pub mod <feature>;` in `routes/mod.rs`.
2. Register in the `protected` router in `routes/mod.rs`.
3. Role gate at the top: `principal.require_admin()?` / `principal.require_designer()?`.

## Workspace context in async tasks

`tokio::spawn` does NOT carry workspace context — the task-locals are lost. Use `crate::spawn_scoped::spawn_scoped` (workspace-preserving) or `spawn_system` (system-bypass) instead. `scope_stream` wraps an `Sse<Stream>` so each `poll_next` re-enters the captured scope; SSE handlers must use it, otherwise stream-side store calls run with no `WORKSPACE_ID`.

## DTO naming

Action DTOs follow `Verb + Noun + (Request|Response)` — `CreateDashboardRequest`, `UpdateOntologyDraftDecisionsRequest`, `ExtendOntologyDraftResponse`. Read-only data shapes that carry no verb keep their noun-only names — `WorkspaceResponse`, `ErrorResponse`, `AdapterAnalysisResponse`.

## Typed error model — `ApiErrorCode` + `params`

Every error response (HTTP body + SSE `error` event) carries the typed wire shape:

```json
{ "error": { "code": "not_found", "class": "client_error", "params": { "entity": "OntologyDraft" } } }
```

- `code` — `ApiErrorCode` enum (snake_case; `error.rs::ApiErrorCode::as_str`).
- `class` — `client_error` (4xx) or `server_error` (5xx).
- `params` — interpolation values for the FE i18n catalogue at `errors.<code>`.

**The backend never produces user-facing prose** — that's the FE's locale concern. Don't use `AppError::bad_request(format!("..."))` for anything user-facing; the English string lands in `params.detail` and Korean users see English. Adding a new code is four sides:

1. New `ApiErrorCode` variant.
2. Mirror in `as_str` and the `every_variant_has_string_and_class` array.
3. Typed constructor with structured params, not free-form strings.
4. `errors.<code>` templates in `web/messages/{ko,en}.json`.

`pnpm error-code-parity-audit` (CI gate) parses two sources: `ApiErrorCode::as_str` (97 codes, namespace `llm_*` + the broader API set) and entelix `Error::wire_signal` (17 buckets, namespace `tool_<wire_code>`). Both sets must round-trip against both locale bundles. 5xx params stay empty by convention — driver text never reaches the wire body. Operators correlate via the `x-request-id` response header (set by the request-id middleware). `runtime_5xx_redacts_driver_text` pins this.

## Language-neutral wire shape

Backend never produces user-facing prose for infrastructure messages. `AnalysisWarning`, `DesignGate`, error responses carry `class` / `id` enums plus an interpolation-args map; the FE i18n catalogue owns the locale. Content (DB-persisted ontology descriptions, glossary terms, rule rationale) uses `LocalizedText` with explicit translations — that's user-authored data, not platform copy.

## Ontology-draft create flow — `selection` is required and explicit

`POST /api/ontology-drafts` and the sibling `extend` / `reanalyze` endpoints take `selection: AnalyzeSelection` as a **required** field (`{"kind": "all"}` for a full sweep, `{"kind": "subset", "tables": [...]}` for a curated list, `{"kind": "extend", "tables": [...]}` to grow a baseline). There is no implicit default — designers pick deliberately so usage-billed backends (BigQuery, Snowflake) only pay introspection cost on the chosen tables.

Empty `subset` / `extend` lists are rejected by `AnalyzeSelection::validate()` at the request boundary; subset names absent from `list_tables()` produce a `Validation` error naming the missing tables.

## Design gates — single evaluator

Whether the design action may proceed is decided by `ox_ontology::design_gate::evaluate_design_gates`. The same vector serves three callers:

- Backend enforcement — `design_ontology_draft` / `design_ontology_draft_stream` call `enforce_design_gates(report, options)` and reject with structured 422 (`design_gates_unmet`) when a blocking gate is unmet.
- API response — `OntologyDraftView` carries `design_gates: Vec<DesignGate>` so the FE renders the checklist without re-deriving rules.
- FE checklist — `DesignGateChecklist` renders one row per gate with i18n keyed by `gate.id` and params from `gate.params`.

Adding a gate = one `GateId` variant + one match arm in `evaluate_design_gates` + one i18n key.

## Model management

`DbModelRouter` implements `ModelResolver`, reads from DB with a 30s TTL cache. After any model-config write, call `state.model_router.invalidate().await` and `state.chat_model_registry.invalidate_all()`.

## Policy registry — entelix multi-tenant glue

`AppState::policy_registry: Arc<entelix::PolicyRegistry>` is wired into both the chat model layer (`ChatModelRegistry::with_policy_registry`) and the agent's tool registry (via `BuildAgentRequest::policy_registry`). One shared `Arc` so `PolicyRegistry::mutate_fallback` reseeds reach both surfaces atomically.

`build_policy_registry` (called at server boot) loads the active pricing catalogue from PostgreSQL through `PostgresStore::list_active_model_prices`, converts it via `ox_brain::pricing::pricing_table_from`, and wraps a `CostMeter` (with `UnknownModelPolicy::WarnOnce` + `UnknownModelMetricsSink` so catalogue drift lights up Prometheus). Conversion failures (NaN / Infinity tariff rows) refuse boot — silent zero would mask catalogue corruption.

`ox_agent::build_execution_context(caps, thread_id, workspace_id)` stamps the workspace as the `entelix::TenantId` so the PolicyLayer cost ledger / quota gate keys per workspace. Background callers (cron, MCP, admin routes) pass `ExecutionContext::default()` and fall onto the `default` tenant.

## Chat streaming

`POST /api/chat/stream` returns SSE. `model_override` swaps the resolved model id before agent build. The agent runs on `Agent::execute(initial_state, &ctx)`; per-event delivery rides a `ChannelSink<ReActState>` folded into the agent's fan-out, and `agent_event_to_payload` projects the canonical lifecycle onto SSE frames:

- `Started` / `ToolStart` / `ToolComplete` / `Complete` — pass-through wire shape.
- `Failed { envelope: LlmFailureEnvelope }` — LLM dispatch failed. `code` is `ApiErrorCode::Llm*`, FE renders prose through `errors.<code>`.
- `ToolError { envelope: ToolFailureEnvelope }` — tool dispatch failed. `wire_code` is the entelix bucket verbatim, FE renders prose through `errors.tool_<wire_code>`.

Both failure envelopes carry `retry_after_secs` + `provider_status` for vendor-aware FE rate-limit handlers. `project_llm_envelope` / `project_tool_envelope` are the single matching helpers — never inline the projection.

## MCP server

`mcp.rs` exposes ontology tools via the `rmcp` crate. Custom domain logic — the MCP surface is its own server, distinct from entelix's `entelix-mcp` client integration.

## Collaboration WebSocket

`/ws/collab` is the only non-HTTP route. `ClientMessage` (client → server) and `ServerMessage` (server → client) are split at the type level so neither side can produce the other's frames; both ride OpenAPI `components/schemas` so the FE generated types pick up new variants automatically.

Auth is one-shot: the first frame within `AUTH_TIMEOUT` (5s) MUST be `ClientMessage::Authenticate { token, workspace_id }`. After validation the connection runs inside `WORKSPACE_ID` + `GRAPH_WORKSPACE_ID` task-locals — every store / graph call rejects cross-workspace identifiers automatically.

Entity locks have a TTL (`collaboration.lock_ttl_secs`); stale locks are reaped on contention. Cursor events are throttled at the hub (`collaboration.cursor_throttle_ms` per user-room pair). Adding a message = extend `ClientMessage` / `ServerMessage` + a `Hub` method + a match arm in `ws::serve_collab`.

## Cron tasks

DB-write sweeps (stale-concept, quality-baseline, soft-delete compaction, draft-checkpoint cleanup) override `CronTask::singleton_key()` to return `Some(ADVISORY_LOCK_CRON_<NAME>)`. The scheduler wraps each tick in `pg_try_advisory_lock`; only the holding replica runs the sweep, others skip silently. In-process-state tasks (clarification evict, collaboration idle reap) leave `singleton_key()` `None` — every replica runs on its own memory.

`spawn_cron(task, Some(pool), cancel)` is production; `spawn_cron(task, None, cancel)` is test-only.

## Notifications

Webhook fan-out lives in `crate::notifications`. Every event implements `EventPayload` (`event_type` / `subject` / `render(channel)`); the generic `dispatch_event<P>` over `&dyn NotificationStore` lists subscribed channels, posts via the shared `WEBHOOK_CLIENT`, and persists a `NotificationLog` row per delivery. Payload renderers branch on `channel.channel_type` and clamp Slack section bodies through `clamp_slack_text` (Slack's 3000-char limit). The SSRF guard parses through `url::Host` + `IpAddr` — never prefix-string heuristics. See `notifications.rs` module docs for the full contract.
