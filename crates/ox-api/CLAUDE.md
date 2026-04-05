# ox-api

Axum HTTP server. Binary name: `ontosyx`.

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
