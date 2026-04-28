# ox-api

Axum HTTP server. Binary name: `ontosyx`.

## Project-create flow — `selection` is required and explicit

`POST /api/projects` and the sibling `extend` / `reanalyze` endpoints take `selection: AnalyzeSelection` as a **required** field (`{"kind": "all"}` for a full sweep, `{"kind": "subset", "tables": [...]}` for a curated list, `{"kind": "extend", "tables": [...]}` to grow an existing baseline). There is no implicit full-warehouse default — designers pick deliberately so usage-billed backends (BigQuery, Snowflake) only pay introspection cost on the chosen tables.

Empty `subset` / `extend` lists are rejected by `AnalyzeSelection::validate()` at the request boundary; subset names that don't appear in `list_tables()` produce a `Validation` error naming the missing tables (no silent drop).

Frontends drive a two-phase flow: `POST /api/projects/source-preview` returns the cheap dataset listing, the user curates with `<TableSelector>`, then `POST /api/projects` is called with the explicit subset.

## DTO naming

Action DTOs follow `Verb + Noun + (Request|Response)` — e.g. `CreateProjectRequest`, `UpdateProjectDecisionsRequest`, `PreviewSourceRequest`, `ExtendProjectResponse`. Read-only data shapes that carry no verb (single-resource responses, error envelopes, summary blocks) keep their noun-only names — `WorkspaceResponse`, `ErrorResponse`, `AdapterAnalysisResponse`, `FederationHealthResponse`.

## Design gates — single evaluator, single source of truth

The design action is the only LLM-driven entry point that consumes operator review state (column clarifications, partial-analysis acknowledgement, large-schema acknowledgement). Whether it may proceed is decided by a single evaluator, `ox_ontology::design_gate::evaluate_design_gates`. The same vector serves three callers:

- **Backend enforcement** — `design_project` and `design_project_stream` call `enforce_design_gates(report, options)` and reject with a structured 422 (`code: "design_gates_unmet"`) when any blocking gate is unmet. Other ontology actions don't gate: `extend` runs analyze+design as a single call so there is no review checkpoint, `reanalyze` doesn't invoke the LLM, and `refine` / `edit` operate on the already-validated ontology rather than the analysis report.
- **API response** — `ProjectView` (the wrapper every project-returning endpoint emits) carries `design_gates: Vec<DesignGate>` so the FE renders the checklist without re-deriving the rules.
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
