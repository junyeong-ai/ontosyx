import { describe, it, expect, vi, beforeEach } from "vitest";
import { chatStream } from "./chat";

// Helper: create a mock SSE response from event/data pairs
function mockSSEResponse(events: Array<{ event: string; data: unknown }>): Response {
  const lines = events.flatMap(({ event, data }) => [
    `event: ${event}`,
    `data: ${JSON.stringify(data)}`,
    "",
  ]);
  const text = lines.join("\n");
  const encoder = new TextEncoder();
  const stream = new ReadableStream({
    start(controller) {
      controller.enqueue(encoder.encode(text));
      controller.close();
    },
  });
  return new Response(stream, { status: 200, headers: { "Content-Type": "text/event-stream" } });
}

function mockErrorResponse(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), { status, headers: { "Content-Type": "application/json" } });
}

const BASE_REQUEST = {
  message: "test",
  ontology: { id: "o1", name: "Test", version: 1, node_types: [], edge_types: [] },
};

describe("chatStream", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    // Mock localStorage for getPrincipalId
    Object.defineProperty(window, "localStorage", {
      value: {
        getItem: vi.fn().mockReturnValue("test-principal"),
        setItem: vi.fn(),
        removeItem: vi.fn(),
      },
      writable: true,
    });
  });

  it("dispatches text events", async () => {
    const textDeltas: string[] = [];
    vi.spyOn(globalThis, "fetch").mockResolvedValue(
      mockSSEResponse([
        { event: "text", data: { delta: "Hello " } },
        { event: "text", data: { delta: "world" } },
        { event: "complete", data: { session_id: "s1", text: "Hello world", tool_calls: 0, iterations: 1 } },
      ]),
    );

    await chatStream(BASE_REQUEST, {
      onText: (delta) => textDeltas.push(delta),
    });

    expect(textDeltas).toEqual(["Hello ", "world"]);
  });

  it("dispatches tool_start and tool_complete events", async () => {
    const starts: string[] = [];
    const completes: string[] = [];
    vi.spyOn(globalThis, "fetch").mockResolvedValue(
      mockSSEResponse([
        { event: "tool_start", data: { id: "t1", name: "query_graph", input: {} } },
        { event: "tool_complete", data: { id: "t1", name: "query_graph", output: "{}", is_error: false, duration_ms: 100 } },
        { event: "complete", data: { session_id: "s1", text: "", tool_calls: 1, iterations: 1 } },
      ]),
    );

    await chatStream(BASE_REQUEST, {
      onToolStart: (e) => starts.push(e.name),
      onToolComplete: (e) => completes.push(e.name),
    });

    expect(starts).toEqual(["query_graph"]);
    expect(completes).toEqual(["query_graph"]);
  });

  it("dispatches thinking events", async () => {
    const thoughts: string[] = [];
    vi.spyOn(globalThis, "fetch").mockResolvedValue(
      mockSSEResponse([
        { event: "thinking", data: { content: "Let me analyze..." } },
        { event: "complete", data: { session_id: "s1", text: "", tool_calls: 0, iterations: 1 } },
      ]),
    );

    await chatStream(BASE_REQUEST, {
      onThinking: (content) => thoughts.push(content),
    });

    expect(thoughts).toEqual(["Let me analyze..."]);
  });

  it("dispatches usage events", async () => {
    let usage: { input_tokens: number; output_tokens: number } | null = null;
    vi.spyOn(globalThis, "fetch").mockResolvedValue(
      mockSSEResponse([
        { event: "usage", data: { input_tokens: 100, output_tokens: 50 } },
        { event: "complete", data: { session_id: "s1", text: "", tool_calls: 0, iterations: 1 } },
      ]),
    );

    await chatStream(BASE_REQUEST, {
      onUsage: (e) => { usage = e; },
    });

    expect(usage).toEqual({ input_tokens: 100, output_tokens: 50 });
  });

  it("calls onComplete with session info", async () => {
    let completed = false;
    let sessionId = "";
    vi.spyOn(globalThis, "fetch").mockResolvedValue(
      mockSSEResponse([
        { event: "complete", data: { session_id: "sess-abc", text: "Done", tool_calls: 2, iterations: 3 } },
      ]),
    );

    await chatStream(BASE_REQUEST, {
      onComplete: (e) => { completed = true; sessionId = e.session_id; },
    });

    expect(completed).toBe(true);
    expect(sessionId).toBe("sess-abc");
  });

  it("calls onError for HTTP errors", async () => {
    let errorMsg = "";
    vi.spyOn(globalThis, "fetch").mockResolvedValue(
      mockErrorResponse(500, { error: { message: "Internal error" } }),
    );

    await chatStream(BASE_REQUEST, {
      onError: (e) => { errorMsg = e; },
    });

    expect(errorMsg).toBe("Internal error");
  });

  it("calls onError for stream-level error events", async () => {
    let errorMsg = "";
    vi.spyOn(globalThis, "fetch").mockResolvedValue(
      mockSSEResponse([
        { event: "text", data: { delta: "partial..." } },
        { event: "error", data: { error: { message: "Context limit exceeded" } } },
      ]),
    );

    await chatStream(BASE_REQUEST, {
      onError: (e) => { errorMsg = e; },
    });

    expect(errorMsg).toBe("Context limit exceeded");
  });

  it("skips malformed SSE data lines gracefully", async () => {
    // Manually construct a response with malformed JSON
    const text = [
      "event: text",
      "data: {not valid json",
      "",
      "event: text",
      'data: {"delta":"ok"}',
      "",
      "event: complete",
      'data: {"session_id":"s1","text":"ok","tool_calls":0,"iterations":1}',
      "",
    ].join("\n");
    const encoder = new TextEncoder();
    const stream = new ReadableStream({
      start(controller) {
        controller.enqueue(encoder.encode(text));
        controller.close();
      },
    });
    vi.spyOn(globalThis, "fetch").mockResolvedValue(
      new Response(stream, { status: 200 }),
    );

    const deltas: string[] = [];
    await chatStream(BASE_REQUEST, {
      onText: (d) => deltas.push(d),
    });

    expect(deltas).toEqual(["ok"]);
  });

  it("dispatches session_expired events", async () => {
    let expired = false;
    vi.spyOn(globalThis, "fetch").mockResolvedValue(
      mockSSEResponse([
        { event: "session_expired", data: { previous_session_id: "old", message: "Session expired" } },
        { event: "complete", data: { session_id: "new", text: "", tool_calls: 0, iterations: 1 } },
      ]),
    );

    await chatStream(BASE_REQUEST, {
      onSessionExpired: () => { expired = true; },
    });

    expect(expired).toBe(true);
  });
});
