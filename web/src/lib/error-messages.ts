/**
 * User-friendly error messages mapped by backend error.type codes.
 *
 * Backend returns: { "error": { "type": "quality_gate", "message": "..." } }
 * Frontend ApiError captures: error.type, error.message, error.details
 *
 * This mapping provides stable, user-friendly messages by error code.
 * Falls back to original message if no mapping found.
 */

const ERROR_TYPE_MESSAGES: Record<string, string> = {
  // Auth & access
  unauthorized: "Authentication required. Please sign in.",
  forbidden: "You don't have permission for this action.",
  rate_limited: "Too many requests. Please wait a moment and try again.",

  // Resources
  not_found: "The requested resource was not found.",

  // Validation
  bad_request: "Invalid request. Please check your input.",
  validation_error: "Validation failed. Please check the form fields.",
  quality_gate: "Quality check failed. Review and resolve quality gaps before proceeding.",

  // Processing
  ontology_error: "Ontology processing error. The schema may have structural issues.",
  compilation_error: "Query compilation failed. Check your query syntax.",
  conflict: "This resource was modified elsewhere. Please refresh and try again.",
  serialization_error: "Data format error. Please try again.",

  // Infrastructure
  timeout: "Request timed out. This operation may take longer for large schemas.",
  service_unavailable: "Service temporarily unavailable. Please try again shortly.",
  internal_error: "An unexpected error occurred. Please try again or contact support.",
};

/**
 * Get a user-friendly error message from an error type code.
 * Falls back to stripping "Runtime error: " prefix from raw message.
 */
export function errorMessage(errorType: string | undefined, rawMessage: string): string {
  if (errorType && errorType in ERROR_TYPE_MESSAGES) {
    return ERROR_TYPE_MESSAGES[errorType];
  }
  // Fallback: strip common prefixes
  return rawMessage.replace(/^Runtime error:\s*/i, "");
}

// ---------------------------------------------------------------------------
// Tool error patterns → user-friendly messages
// ---------------------------------------------------------------------------

const TOOL_ERROR_PATTERNS: Array<{ pattern: RegExp; message: string }> = [
  {
    pattern: /API error \(HTTP 400\)/i,
    message: "The ontology is too large for this query. Try asking about specific entities.",
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
    message: "Could not translate your question to a graph query. Try rephrasing with specific entity names.",
  },
];

/**
 * Convert a raw tool error output to a user-friendly message.
 * Returns { userMessage, technicalDetail } for progressive disclosure.
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
    userMessage: cleaned.length > 120 ? cleaned.slice(0, 120) + "..." : cleaned,
    technicalDetail: rawOutput,
  };
}
