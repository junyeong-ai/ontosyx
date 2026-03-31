# ox-api

Axum HTTP server. Binary name: `ontosyx`.

## Adding a New Route

1. Create handler in `routes/my_feature.rs`.
2. Add `pub mod my_feature;` in `routes/mod.rs`.
3. Register route in the `protected` router in `routes/mod.rs`.
4. Admin-only: call `principal.require_admin()?` at the start.
5. Workspace-scoped data: RLS handles isolation automatically via middleware.

## Middleware Stack (order matters)

`require_auth` → `workspace_context` → `audit_log` → handler.

## Model Management

- `DbModelRouter` implements `ModelResolver`, reads from DB with 30s TTL cache.
- After any model config change, call `state.model_router.invalidate().await` and `state.client_pool.invalidate_all()`.

## Chat Streaming

- `POST /api/chat/stream` returns SSE events.
- `model_override` field in request → `RunConfig` for per-request model switch.
- Agent uses branchforge's `execute_stream_with()`.
