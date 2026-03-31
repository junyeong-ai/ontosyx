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

  try {
    while (true) {
      if (options?.signal?.aborted) break;

      const { done, value } = await reader.read();
      if (done) break;

      buffer += decoder.decode(value, { stream: true });
      const lines = buffer.split("\n");
      buffer = lines.pop() ?? "";

      for (const line of lines) {
        if (line.startsWith("event: ")) {
          currentEvent = line.slice(7).trim();
          continue;
        }

        if (line.startsWith("data: ")) {
          const raw = line.slice(6);
          if (currentEvent && handlers[currentEvent]) {
            try {
              const data = JSON.parse(raw);
              handlers[currentEvent](data);
            } catch {
              // Skip malformed SSE data
            }
          }
          currentEvent = "";
        }
      }
    }
  } finally {
    reader.cancel().catch(() => {});
  }
}
