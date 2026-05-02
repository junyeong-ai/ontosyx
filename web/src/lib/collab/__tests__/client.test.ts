// CollaborationClient transport tests against a mock WebSocket.
// We exercise the auth handshake, send-before-ready queueing, and
// reconnect-then-rejoin behaviour — the three pieces of contract
// the server depends on the client to honour.

import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

import { CollaborationClient } from "../client";
import type { ServerMessage } from "../types";

class FakeSocket {
  static readonly OPEN = 1;
  static readonly CLOSED = 3;

  static instances: FakeSocket[] = [];
  readyState: number = 0;
  sent: string[] = [];
  listeners: Record<string, ((event: unknown) => void)[]> = {};

  constructor(public url: string) {
    FakeSocket.instances.push(this);
  }

  addEventListener(name: string, fn: (event: unknown) => void): void {
    (this.listeners[name] ??= []).push(fn);
  }

  send(data: string): void {
    this.sent.push(data);
  }

  close(): void {
    this.readyState = FakeSocket.CLOSED;
    this.dispatch("close", new Event("close"));
  }

  // Test helpers
  open(): void {
    this.readyState = FakeSocket.OPEN;
    this.dispatch("open", new Event("open"));
  }

  receive(msg: ServerMessage): void {
    this.dispatch("message", { data: JSON.stringify(msg) });
  }

  private dispatch(name: string, event: unknown): void {
    for (const fn of this.listeners[name] ?? []) fn(event);
  }
}

const originalWebSocket = globalThis.WebSocket;

beforeEach(() => {
  FakeSocket.instances = [];
  // @ts-expect-error — assigning a stub for the test
  globalThis.WebSocket = FakeSocket;
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
  globalThis.WebSocket = originalWebSocket;
});

const baseConfig = (override: Partial<{ token: string }> = {}) => ({
  url: "ws://localhost/ws/collab",
  workspaceId: "00000000-0000-0000-0000-000000000001",
  getToken: vi.fn().mockResolvedValue(override.token ?? "tok"),
  onMessage: vi.fn(),
  onStateChange: vi.fn(),
});

describe("CollaborationClient", () => {
  it("sends Authenticate on socket open", async () => {
    const cfg = baseConfig();
    const client = new CollaborationClient(cfg);
    client.connect();
    await vi.runAllTimersAsync();
    const sock = FakeSocket.instances[0];
    sock.open();

    expect(sock.sent).toHaveLength(1);
    const frame = JSON.parse(sock.sent[0]);
    expect(frame.type).toBe("authenticate");
    expect(frame.token).toBe("tok");
    expect(frame.workspace_id).toBe(cfg.workspaceId);
  });

  it("queues sends before Authenticated arrives", async () => {
    const cfg = baseConfig();
    const client = new CollaborationClient(cfg);
    client.connect();
    await vi.runAllTimersAsync();

    // No socket yet — send queues.
    client.moveCursor("p1", 1, 2, null);

    const sock = FakeSocket.instances[0];
    sock.open();

    // Auth frame is the only one sent until Authenticated.
    expect(sock.sent).toHaveLength(1);

    sock.receive({ type: "authenticated", user_id: "u1", user_name: "Alice" });

    // After auth, the queued cursor frame is flushed.
    expect(sock.sent).toHaveLength(2);
    const cursor = JSON.parse(sock.sent[1]);
    expect(cursor.type).toBe("move_cursor");
  });

  it("re-joins rooms after reconnect", async () => {
    const cfg = baseConfig();
    const client = new CollaborationClient(cfg);
    client.connect();
    await vi.runAllTimersAsync();
    let sock = FakeSocket.instances[0];
    sock.open();
    sock.receive({ type: "authenticated", user_id: "u1", user_name: "Alice" });

    client.join("p1");
    expect(sock.sent.some((s) => s.includes('"join"'))).toBe(true);

    // Drop the socket — reconnect schedules.
    sock.close();
    expect(client.connectionState()).toBe("reconnecting");

    // Advance past the first backoff slot and let the reconnect run.
    await vi.advanceTimersByTimeAsync(1_000);
    await vi.runAllTimersAsync();
    sock = FakeSocket.instances[1];
    sock.open();
    sock.receive({ type: "authenticated", user_id: "u1", user_name: "Alice" });

    // Auth (1) + auto-rejoin (1) = at least 2 frames on the new socket.
    expect(sock.sent.length).toBeGreaterThanOrEqual(2);
    expect(sock.sent.some((s) => s.includes('"join"'))).toBe(true);
  });

  it("disconnect stops further reconnect attempts", async () => {
    const cfg = baseConfig();
    const client = new CollaborationClient(cfg);
    client.connect();
    await vi.runAllTimersAsync();
    const sock = FakeSocket.instances[0];
    sock.open();
    sock.receive({ type: "authenticated", user_id: "u1", user_name: "Alice" });

    client.disconnect();
    await vi.runAllTimersAsync();
    expect(client.connectionState()).toBe("idle");
    // No new sockets after disconnect.
    expect(FakeSocket.instances).toHaveLength(1);
  });
});
