import type { ChatStreamRequest } from "@/types/api";
import { getPrincipalId } from "@/lib/principal";
import { getWorkspaceId } from "@/lib/workspace";
import { fetchWithTimeout, PROXY_BASE, DESIGN_TIMEOUT } from "./client";
import { consumeSSEStream } from "./sse";

// ---------------------------------------------------------------------------
// Agent SSE event types
// ---------------------------------------------------------------------------

export interface AgentTextEvent {
  delta: string;
}

export interface AgentToolStartEvent {
  run_id: string;
  tool_use_id: string;
  tool: string;
  input: unknown;
}

export interface AgentToolCompleteEvent {
  run_id: string;
  tool_use_id: string;
  tool: string;
  output: string;
  duration_ms: number;
}

export interface AgentToolReviewEvent {
  tool_use_id: string;
  tool: string;
  input: unknown;
}

export interface AgentToolProgressEvent {
  tool_call_id: string;
  step: string;
  status: "started" | "completed" | "failed";
  duration_ms?: number;
  metadata?: Record<string, unknown>;
}

export interface AgentCompleteEvent {
  run_id: string;
  text: string;
  steps: number;
  input_tokens?: number;
  output_tokens?: number;
}

export interface AgentSessionExpiredEvent {
  previous_session_id: string;
  message: string;
}

/**
 * LLM dispatch failure envelope — mirrors the backend's
 * `LlmFailureEnvelope`. `code` is `ApiErrorCode::as_str` (always
 * `llm_*`); FE renders prose through `errors.<code>`.
 */
export interface LlmFailureEnvelope {
  code: string;
  class: "client_error" | "server_error";
  retry_after_secs?: number;
  provider_status?: number;
}

/**
 * Tool dispatch failure envelope — mirrors the backend's
 * `ToolFailureEnvelope`. `wire_code` is the entelix bucket
 * (`invalid_request` / `serde` / `cancelled` / …); FE renders
 * prose through `errors.tool_<wire_code>`. Distinct namespace from
 * LLM failures.
 */
export interface ToolFailureEnvelope {
  wire_code: string;
  class: "client_error" | "server_error";
  retry_after_secs?: number;
  provider_status?: number;
}

export interface AgentFailedEvent {
  run_id: string;
  envelope: LlmFailureEnvelope;
  /**
   * Interpolation params for the FE i18n catalogue. Reserved slot
   * — current emissions pass `{}` per the typed-error doctrine
   * (operator detail stays in the server log, never the wire body).
   */
  params: Record<string, unknown>;
}

export interface AgentToolErrorEvent {
  run_id: string;
  tool_use_id: string;
  tool: string;
  error: string;
  error_for_llm: string;
  duration_ms: number;
  envelope: ToolFailureEnvelope;
}

// ---------------------------------------------------------------------------
// Agent streaming callbacks
// ---------------------------------------------------------------------------

export interface StreamCallbacks {
  onText?: (delta: string) => void;
  onThinking?: (content: string) => void;
  onToolStart?: (event: AgentToolStartEvent) => void;
  onToolComplete?: (event: AgentToolCompleteEvent) => void;
  /**
   * Tool dispatch failed. Carries the typed wire envelope
   * (`code` / `class` / `retry_after_secs` / `provider_status`)
   * — consumers resolve prose through `errors.<code>` exactly
   * like `onFailed`. Distinct from `onToolComplete`: success and
   * failure are separate variants on the wire.
   */
  onToolError?: (event: AgentToolErrorEvent) => void;
  onToolProgress?: (event: AgentToolProgressEvent) => void;
  onToolReview?: (event: AgentToolReviewEvent) => void;
  onUsage?: (event: { input_tokens: number; output_tokens: number }) => void;
  onComplete?: (event: AgentCompleteEvent) => void;
  onSessionExpired?: (event: AgentSessionExpiredEvent) => void;
  /**
   * Typed agent failure (run terminated). Falls through to `onError`
   * when undefined so legacy callers still surface something — but
   * production callers wire `onFailed` so the FE i18n catalogue can
   * resolve `errors.<code>` against `event.params`.
   */
  onFailed?: (event: AgentFailedEvent) => void;
  onError?: (error: string) => void;
}

// ---------------------------------------------------------------------------
// SSE error guard
// ---------------------------------------------------------------------------

function handleSseError(
  d: Record<string, unknown>,
  onError?: (message: string) => void,
): boolean {
  if (d.error) {
    const err = d.error as { message?: string };
    onError?.(err.message ?? String(d.error));
    return true;
  }
  return false;
}

// ---------------------------------------------------------------------------
// Chat stream
// ---------------------------------------------------------------------------

export async function chatStream(
  req: ChatStreamRequest,
  callbacks: StreamCallbacks,
  signal?: AbortSignal,
): Promise<void> {
  const headers = new Headers({ "Content-Type": "application/json" });
  const principalId = getPrincipalId();
  if (principalId) {
    headers.set("x-principal-id", principalId);
  }
  const workspaceId = getWorkspaceId();
  if (workspaceId) {
    headers.set("x-workspace-id", workspaceId);
  }

  const res = await fetchWithTimeout(`${PROXY_BASE}/chat/stream`, {
    method: "POST",
    headers,
    body: JSON.stringify(req),
    timeout: DESIGN_TIMEOUT,
  });

  if (!res.ok || !res.body) {
    const body = await res.json().catch(() => ({}));
    callbacks.onError?.(body.error?.message ?? body.error ?? `Stream error ${res.status}`);
    return;
  }

  await consumeSSEStream(
    res,
    {
      text: (data) => {
        const d = data as Record<string, unknown> & { delta: string };
        if (handleSseError(d, callbacks.onError)) return;
        callbacks.onText?.(d.delta);
      },
      thinking: (data) => {
        const d = data as Record<string, unknown> & { content: string };
        if (handleSseError(d, callbacks.onError)) return;
        callbacks.onThinking?.(d.content);
      },
      tool_start: (data) => {
        const d = data as Record<string, unknown> & AgentToolStartEvent;
        if (handleSseError(d, callbacks.onError)) return;
        callbacks.onToolStart?.(d);
      },
      tool_complete: (data) => {
        const d = data as Record<string, unknown> & AgentToolCompleteEvent;
        if (handleSseError(d, callbacks.onError)) return;
        callbacks.onToolComplete?.(d);
      },
      tool_error: (data) => {
        const d = data as Record<string, unknown> & AgentToolErrorEvent;
        if (callbacks.onToolError) {
          callbacks.onToolError(d);
          return;
        }
        callbacks.onError?.(`errors.tool_${d.envelope.wire_code}`);
      },
      tool_progress: (data) => {
        const d = data as Record<string, unknown> & AgentToolProgressEvent;
        if (handleSseError(d, callbacks.onError)) return;
        callbacks.onToolProgress?.(d);
      },
      tool_review: (data) => {
        const d = data as Record<string, unknown> & AgentToolReviewEvent;
        if (handleSseError(d, callbacks.onError)) return;
        callbacks.onToolReview?.(d);
      },
      usage: (data) => {
        const d = data as Record<string, unknown> & { input_tokens: number; output_tokens: number };
        if (handleSseError(d, callbacks.onError)) return;
        callbacks.onUsage?.(d);
      },
      complete: (data) => {
        const d = data as Record<string, unknown> & AgentCompleteEvent;
        if (handleSseError(d, callbacks.onError)) return;
        callbacks.onComplete?.(d);
      },
      session_expired: (data) => {
        const d = data as Record<string, unknown> & AgentSessionExpiredEvent;
        if (handleSseError(d, callbacks.onError)) return;
        callbacks.onSessionExpired?.(d);
      },
      failed: (data) => {
        const d = data as Record<string, unknown> & AgentFailedEvent;
        if (callbacks.onFailed) {
          callbacks.onFailed(d);
          return;
        }
        callbacks.onError?.(`errors.${d.envelope.code}`);
      },
      error: (data) => {
        const d = data as Record<string, unknown>;
        if (!handleSseError(d, callbacks.onError)) {
          callbacks.onError?.("Unknown error");
        }
      },
    },
    { signal, onError: callbacks.onError },
  );
}
