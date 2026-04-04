import type { ChatStreamRequest } from "@/types/api";
import { getPrincipalId } from "@/lib/principal";
import { fetchWithTimeout, PROXY_BASE, DESIGN_TIMEOUT } from "./client";
import { consumeSSEStream } from "./sse";

// ---------------------------------------------------------------------------
// Agent SSE event types
// ---------------------------------------------------------------------------

export interface AgentTextEvent {
  delta: string;
}

export interface AgentToolStartEvent {
  id: string;
  name: string;
  input: unknown;
}

export interface AgentToolCompleteEvent {
  id: string;
  name: string;
  output: string;
  is_error: boolean;
  duration_ms: number;
}

export interface AgentToolReviewEvent {
  id: string;
  name: string;
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
  session_id: string;
  text: string;
  tool_calls: number;
  iterations: number;
}

export interface AgentSessionExpiredEvent {
  previous_session_id: string;
  message: string;
}

// ---------------------------------------------------------------------------
// Agent streaming callbacks
// ---------------------------------------------------------------------------

export interface StreamCallbacks {
  onText?: (delta: string) => void;
  onThinking?: (content: string) => void;
  onToolStart?: (event: AgentToolStartEvent) => void;
  onToolComplete?: (event: AgentToolCompleteEvent) => void;
  onToolProgress?: (event: AgentToolProgressEvent) => void;
  onToolReview?: (event: AgentToolReviewEvent) => void;
  onUsage?: (event: { input_tokens: number; output_tokens: number }) => void;
  onComplete?: (event: AgentCompleteEvent) => void;
  onSessionExpired?: (event: AgentSessionExpiredEvent) => void;
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
      error: (data) => {
        const d = data as Record<string, unknown>;
        handleSseError(d, callbacks.onError) ||
          callbacks.onError?.("Unknown error");
      },
    },
    { signal, onError: callbacks.onError },
  );
}
