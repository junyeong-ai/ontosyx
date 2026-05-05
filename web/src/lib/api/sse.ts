/**
 * Consume an SSE stream from a fetch Response, dispatching parsed events to handlers.
 *
 * Each handler key corresponds to an SSE `event:` name. When a matching event
 * arrives, the handler is called with the JSON-parsed `data:` payload (or the
 * raw string if parsing fails).
 *
 * Returns when the stream ends or the optional AbortSignal fires.
 */
export async function consumeSSEStream(
  response: Response,
  handlers: Record<string, (data: unknown) => void>,
  options?: {
    signal?: AbortSignal;
    onError?: (message: string) => void;
  },
): Promise<void> {
  const body = response.body;
  if (!body) {
    options?.onError?.("No response body");
    return;
  }

  const reader = body.getReader();
  const decoder = new TextDecoder();
  let buffer = "";
  let currentEvent = "";

  /**
   * Process a single complete line. Hoisted out of the read loop so
   * the trailing-buffer flush at end-of-stream uses the same parsing
   * path — a server that closes the socket without a final blank line
   * (network truncation, abrupt shutdown) still has its last
   * `event:`/`data:` pair dispatched.
   */
  const handleLine = (line: string) => {
    if (line.startsWith("event: ")) {
      currentEvent = line.slice(7).trim();
      return;
    }
    if (line.startsWith("data: ")) {
      const raw = line.slice(6);
      if (currentEvent && handlers[currentEvent]) {
        try {
          handlers[currentEvent](JSON.parse(raw));
        } catch {
          // Skip malformed SSE data
        }
      }
      currentEvent = "";
    }
  };

  try {
    while (true) {
      if (options?.signal?.aborted) break;

      const { done, value } = await reader.read();
      if (done) break;

      buffer += decoder.decode(value, { stream: true });
      const lines = buffer.split("\n");
      buffer = lines.pop() ?? "";

      for (const line of lines) handleLine(line);
    }

    // End-of-stream flush. The decoder may still hold a partial UTF-8
    // sequence; calling without `{ stream: true }` finalises it. Any
    // residue in `buffer` after the read loop is the stream's final
    // line — process it so an abrupt close (no terminating blank
    // line) doesn't drop the last event.
    buffer += decoder.decode();
    if (buffer.length > 0) handleLine(buffer);
  } finally {
    reader.cancel().catch(() => {});
  }
}
