// CollaborationClient — transport layer for the realtime WS
// protocol. Owns the socket lifecycle, the auth handshake, and
// reconnection. Keeps zero domain state — UI consumers attach a
// `onMessage` handler and project-scoped state lives in
// `useCollabStore`.

import type { ClientMessage, ServerMessage } from "./types";

/**
 * Exponential backoff schedule (ms) for the reconnect timer. The
 * tail caps at 30 s so a long outage doesn't park the client at a
 * silently absurd interval. Past the last entry we keep retrying
 * at 30 s until the user navigates away.
 */
const RECONNECT_DELAYS_MS = [1_000, 2_000, 4_000, 8_000, 15_000, 30_000];

/**
 * Cap on the pre-auth send queue. A user clicking 100 cursors
 * during a 30s outage shouldn't burst-flush all of them on
 * reconnect — drop the oldest, keep the freshest signal. Cursor
 * traffic is lossy by design (server-side throttle ignores most
 * of a flood anyway); locks and join/leave are rare enough to
 * never approach this ceiling in practice.
 */
const MAX_OUTBOX_SIZE = 64;

export type ConnectionState =
  | "idle"
  | "connecting"
  | "authenticating"
  | "ready"
  | "reconnecting"
  | "closed";

export interface CollaborationClientConfig {
  /** Absolute or path-relative WS URL — e.g. `ws://host/ws/collab`. */
  url: string;
  workspaceId: string;
  /**
   * Token provider. Called on every (re)connect so the client
   * picks up rotated / refreshed JWTs without recreating the
   * socket. Throw / reject to give up reconnecting.
   */
  getToken(): string | Promise<string>;

  onMessage(msg: ServerMessage): void;
  onStateChange?(state: ConnectionState): void;
}

/**
 * Single WebSocket connection bound to one workspace. The class is
 * intentionally framework-agnostic — the React hook
 * (`useCollab`) wires it into the Zustand store and lifecycle.
 */
export class CollaborationClient {
  private socket: WebSocket | null = null;
  private state: ConnectionState = "idle";
  private outbox: ClientMessage[] = [];
  private rejoinSet = new Set<string>();
  private reconnectAttempt = 0;
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  private explicitlyClosed = false;

  constructor(private readonly config: CollaborationClientConfig) {}

  /** Current public state. */
  connectionState(): ConnectionState {
    return this.state;
  }

  /** Open the socket. Idempotent — re-call after `disconnect()`. */
  connect(): void {
    if (this.state === "connecting" || this.state === "ready") {
      return;
    }
    this.explicitlyClosed = false;
    this.openSocket();
  }

  private async openSocket(): Promise<void> {
    this.transition("connecting");
    let token: string;
    try {
      token = await this.config.getToken();
    } catch {
      // Token provider gave up — surface a SessionRevoked-equivalent
      // through onMessage so the UI can prompt a fresh login.
      this.config.onMessage({
        type: "error",
        code: "session_revoked",
        params: {},
      });
      this.transition("closed");
      return;
    }

    const socket = new WebSocket(this.config.url);
    this.socket = socket;
    socket.addEventListener("open", () => this.handleOpen(token));
    socket.addEventListener("message", (event) =>
      this.handleMessage(event.data),
    );
    socket.addEventListener("close", () => this.handleClose());
    socket.addEventListener("error", () => {
      // 'error' is always followed by 'close' in the browser; let
      // the close handler drive reconnection so the logic lives in
      // one place.
    });
  }

  private handleOpen(token: string): void {
    this.transition("authenticating");
    const auth: ClientMessage = {
      type: "authenticate",
      token,
      workspace_id: this.config.workspaceId,
    };
    this.rawSend(auth);
  }

  private handleMessage(data: unknown): void {
    if (typeof data !== "string") return;
    let msg: ServerMessage;
    try {
      msg = JSON.parse(data) as ServerMessage;
    } catch {
      return;
    }

    if (msg.type === "authenticated") {
      this.transition("ready");
      this.reconnectAttempt = 0;

      // Re-join every room we were in before the disconnect, then
      // flush queued client messages in arrival order.
      for (const ontologyDraftId of this.rejoinSet) {
        this.rawSend({ type: "join", ontology_draft_id: ontologyDraftId });
      }
      const queued = this.outbox.splice(0);
      for (const m of queued) this.rawSend(m);
    }

    this.config.onMessage(msg);
  }

  private handleClose(): void {
    this.socket = null;
    if (this.explicitlyClosed) {
      this.transition("closed");
      return;
    }
    this.scheduleReconnect();
  }

  private scheduleReconnect(): void {
    if (this.reconnectTimer) return;
    this.transition("reconnecting");
    const idx = Math.min(
      this.reconnectAttempt,
      RECONNECT_DELAYS_MS.length - 1,
    );
    const delay = RECONNECT_DELAYS_MS[idx];
    this.reconnectAttempt += 1;
    this.reconnectTimer = setTimeout(() => {
      this.reconnectTimer = null;
      void this.openSocket();
    }, delay);
  }

  /**
   * Send a `ClientMessage`. Frames sent before `Authenticated`
   * arrives are queued and flushed on auth success — call sites
   * don't need to await readiness.
   *
   * The queue caps at [`MAX_OUTBOX_SIZE`]. When full, the policy
   * is to drop the oldest `move_cursor` first — cursor data is
   * lossy by design at the server's throttle anyway, so dropping
   * one preserves the at-most-once semantics that lock and
   * join/leave frames need. If no cursor is queued (rare: the
   * user only sent control frames during the outage), fall back
   * to dropping the oldest entry so the queue can't grow
   * unbounded.
   */
  send(msg: ClientMessage): void {
    if (this.state === "ready" && this.socket?.readyState === WebSocket.OPEN) {
      this.rawSend(msg);
      return;
    }
    if (this.outbox.length >= MAX_OUTBOX_SIZE) {
      const idx = this.outbox.findIndex((m) => m.type === "move_cursor");
      if (idx >= 0) this.outbox.splice(idx, 1);
      else this.outbox.shift();
    }
    this.outbox.push(msg);
  }

  /** Convenience — record the room so reconnect re-joins it. */
  join(ontologyDraftId: string): void {
    this.rejoinSet.add(ontologyDraftId);
    this.send({ type: "join", ontology_draft_id: ontologyDraftId });
  }

  leave(ontologyDraftId: string): void {
    this.rejoinSet.delete(ontologyDraftId);
    this.send({ type: "leave", ontology_draft_id: ontologyDraftId });
  }

  moveCursor(
    ontologyDraftId: string,
    x: number,
    y: number,
    selectedElement: string | null,
  ): void {
    this.send({
      type: "move_cursor",
      ontology_draft_id: ontologyDraftId,
      x,
      y,
      selected_element: selectedElement,
    });
  }

  acquireLock(ontologyDraftId: string, entityId: string): void {
    this.send({
      type: "acquire_lock",
      ontology_draft_id: ontologyDraftId,
      entity_id: entityId,
    });
  }

  releaseLock(ontologyDraftId: string, entityId: string): void {
    this.send({
      type: "release_lock",
      ontology_draft_id: ontologyDraftId,
      entity_id: entityId,
    });
  }

  /**
   * Close the socket and stop reconnecting. After `disconnect()`
   * the client is back in the `idle` state and `connect()` may be
   * called again.
   */
  disconnect(): void {
    this.explicitlyClosed = true;
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
      this.reconnectTimer = null;
    }
    this.socket?.close();
    this.socket = null;
    this.outbox = [];
    this.rejoinSet.clear();
    this.reconnectAttempt = 0;
    this.transition("idle");
  }

  private rawSend(msg: ClientMessage): void {
    if (this.socket?.readyState === WebSocket.OPEN) {
      this.socket.send(JSON.stringify(msg));
    }
  }

  private transition(state: ConnectionState): void {
    if (this.state === state) return;
    this.state = state;
    this.config.onStateChange?.(state);
  }
}
