// Tool-error pattern matcher — translates raw chat-tool stderr into
// progressive-disclosure messages. Distinct from the typed
// `ApiError.localize(t)` path: tool output is unstructured stdout
// from agent-driven shell processes, so the localisation has to
// pattern-match on substrings rather than read a typed code.
//
// HTTP errors live on `ApiError.localize(t)` reading the
// `errors.<code>` i18n catalog — see `lib/api/client.ts` and the
// `ApiErrorCode` enum on the backend. Don't add new entries here for
// HTTP errors.

const TOOL_ERROR_PATTERNS: Array<{ pattern: RegExp; message: string }> = [
  {
    pattern: /API error \(HTTP 400\)/i,
    message:
      "The ontology is too large for this query. Try asking about specific entities.",
  },
  {
    pattern: /token limit|too long|max_tokens/i,
    message: "The response was too large. Try a more specific question.",
  },
  {
    pattern: /Unable to find image|pull access denied/i,
    message: "Analysis environment not configured. Contact your administrator.",
  },
  {
    pattern: /Connection refused|connection reset/i,
    message: "Database connection failed. The service may be restarting.",
  },
  {
    pattern: /timed out|timeout/i,
    message: "The operation timed out. Try a simpler query.",
  },
  {
    pattern: /Query translation failed/i,
    message:
      "Could not translate your question to a graph query. Try rephrasing with specific entity names.",
  },
];

/**
 * Convert a raw tool error output to a user-friendly message.
 * Returns `{ userMessage, technicalDetail }` for progressive
 * disclosure — the message is the chat bubble copy, the detail is
 * the developer-tools-style raw output that lives behind a "show
 * details" toggle.
 */
export function toolErrorMessage(rawOutput: string): {
  userMessage: string;
  technicalDetail: string;
} {
  for (const { pattern, message } of TOOL_ERROR_PATTERNS) {
    if (pattern.test(rawOutput)) {
      return { userMessage: message, technicalDetail: rawOutput };
    }
  }
  // No pattern matched — strip common prefixes
  const cleaned = rawOutput
    .replace(/^execution failed:\s*/i, "")
    .replace(/^Runtime error:\s*/i, "");
  return {
    userMessage:
      cleaned.length > 120 ? `${cleaned.slice(0, 120)}...` : cleaned,
    technicalDetail: rawOutput,
  };
}
