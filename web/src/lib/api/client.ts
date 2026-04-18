import { getPrincipalId } from "@/lib/principal";
import { getWorkspaceId } from "@/lib/workspace";
import type { components } from "@/types/api.generated";

// Sample type-only import from the generated OpenAPI types. Proves that the
// code-gen pipeline (scripts/gen-openapi-types.sh) wires the backend spec
// into the frontend type system. Reference this from real call sites as we
// migrate handcrafted types (src/types/api.ts etc.) to generated ones.
export type GeneratedErrorResponse = components["schemas"]["ErrorResponse"];

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

export const PROXY_BASE = "/api/proxy";
export const DEFAULT_TIMEOUT = 30_000; // 30s for regular calls
export const DESIGN_TIMEOUT = 120_000; // 120s for design/LLM operations

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export type FetchOptions = RequestInit & { timeout?: number };
export type RetryOptions = FetchOptions & { maxRetries?: number };

// ---------------------------------------------------------------------------
// Core HTTP utilities
// ---------------------------------------------------------------------------

export async function fetchWithTimeout(
  url: string,
  options: FetchOptions = {},
): Promise<Response> {
  const { timeout = DEFAULT_TIMEOUT, ...fetchOptions } = options;
  const controller = new AbortController();
  const id = setTimeout(() => controller.abort(), timeout);

  try {
    const response = await fetch(url, {
      ...fetchOptions,
      signal: controller.signal,
    });
    return response;
  } finally {
    clearTimeout(id);
  }
}

/**
 * One-shot fetch with two narrow retry cases that MUST live below the
 * TanStack layer:
 *
 * 1. **429 Too Many Requests** — honours the `Retry-After` header via a
 *    low-level wait before re-sending. The upper query layer cannot see
 *    the header, so retrying here is the only way to respect rate-limit
 *    guidance.
 * 2. **Network abort / DOMException** — surfaces as an Error so the
 *    caller (and ultimately TanStack) sees a consistent thrown error
 *    shape rather than a rejected fetch Promise.
 *
 * 5xx responses are intentionally NOT retried here. Retrying at both
 * this layer and the TanStack `queries.retry` callback would compound
 * (up to 5 × 2 attempts per user request) and flatten a server outage
 * into a slow cascade of backed-off failures. Let TanStack own server-
 * error retries so the policy is observable in one place.
 */
export async function fetchWithRetry(
  url: string,
  options: RetryOptions = {},
): Promise<Response> {
  const { maxRetries = 2, ...fetchOptions } = options;
  let lastError: Error | null = null;

  for (let attempt = 0; attempt <= maxRetries; attempt++) {
    try {
      const response = await fetchWithTimeout(url, fetchOptions);

      // 429 — honour Retry-After before retrying.
      if (response.status === 429 && attempt < maxRetries) {
        const retryAfter = response.headers.get("retry-after");
        const waitMs = retryAfter
          ? (parseInt(retryAfter, 10) || 2) * 1000
          : Math.min(1000 * 2 ** attempt, 8000);
        await new Promise((r) => setTimeout(r, waitMs));
        continue;
      }

      // Everything else (2xx, 4xx, 5xx, and the final 429) is returned
      // as-is. `requestInternal` wraps non-2xx in `ApiError` and TanStack
      // decides whether to retry based on `ApiError.status`.
      return response;
    } catch (err) {
      if (err instanceof DOMException && err.name === "AbortError") {
        lastError = new Error("Request timed out");
      } else {
        lastError = err as Error;
      }
      if (attempt < maxRetries) {
        await new Promise((r) => setTimeout(r, 100 * Math.pow(2, attempt)));
      }
    }
  }

  throw lastError ?? new Error("Request failed");
}

// ---------------------------------------------------------------------------
// ApiError
// ---------------------------------------------------------------------------

export class ApiError extends Error {
  /** HTTP status code from the failing response, or 0 when unknown
   * (network error, aborted request, non-HTTP throw). Prefer this over
   * regex-matching the message string when deciding retry eligibility. */
  status: number;
  type?: string;
  details?: unknown;

  constructor(
    message: string,
    options?: { status?: number; type?: string; details?: unknown },
  ) {
    super(message);
    this.name = "ApiError";
    this.status = options?.status ?? 0;
    this.type = options?.type;
    this.details = options?.details;
  }

  /** Non-retryable client error (4xx). */
  isClientError(): boolean {
    return this.status >= 400 && this.status < 500;
  }
}

// ---------------------------------------------------------------------------
// Internal request helpers (exported for sibling modules, NOT from barrel)
// ---------------------------------------------------------------------------

async function requestInternal<T>(
  path: string,
  init: RetryOptions | undefined,
  parseResponse: (res: Response) => Promise<T>,
): Promise<T> {
  const headers = new Headers(init?.headers);
  headers.set("Content-Type", "application/json");

  const principalId = getPrincipalId();
  if (principalId) {
    headers.set("x-principal-id", principalId);
  }

  const workspaceId = getWorkspaceId();
  if (workspaceId) {
    headers.set("x-workspace-id", workspaceId);
  }

  const { timeout, maxRetries, ...fetchInit } = init ?? {};
  const res = await fetchWithRetry(`${PROXY_BASE}${path}`, {
    ...fetchInit,
    headers,
    timeout: timeout ?? DESIGN_TIMEOUT,
    maxRetries,
  });

  if (!res.ok) {
    const body = await res.json().catch(() => ({}));
    throw new ApiError(
      body.error?.message ?? body.error ?? `API error ${res.status}`,
      {
        status: res.status,
        type: body.error?.type,
        details: body.error?.details,
      },
    );
  }

  return parseResponse(res);
}

export async function request<T>(path: string, init?: RetryOptions): Promise<T> {
  return requestInternal(path, init, async (res) => {
    if (res.status === 204) {
      return undefined as T;
    }
    const body = await res.json();
    return unwrapEnvelope<T>(body);
  });
}

export async function requestText(path: string, init?: RetryOptions): Promise<string> {
  return requestInternal(path, init, (res) => res.text());
}

/**
 * Unwrap the `ApiResponse<T>` envelope: `{ data, pagination?, meta? }`.
 *
 * Cursor-paginated payloads are flattened back to the legacy
 * `{ items, next_cursor }` shape so existing list components keep
 * working without per-component edits. Single-resource responses
 * return `data` directly.
 *
 * Defensive fallback: if the body isn't an envelope (legacy bare JSON,
 * mocked tests, or third-party endpoints), return it as-is.
 */
function unwrapEnvelope<T>(body: unknown): T {
  if (
    body === null ||
    typeof body !== "object" ||
    !Object.prototype.hasOwnProperty.call(body, "data")
  ) {
    return body as T;
  }

  const obj = body as { data: unknown; pagination?: { next_cursor?: string | null } };

  // Cursor-paginated: backend `{ data: [...], pagination: { next_cursor } }`
  // → frontend `{ items: [...], next_cursor }`.
  if (Array.isArray(obj.data) && obj.pagination !== undefined) {
    return {
      items: obj.data,
      next_cursor: obj.pagination.next_cursor ?? null,
    } as T;
  }

  return obj.data as T;
}
