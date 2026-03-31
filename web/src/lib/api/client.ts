import { getPrincipalId } from "@/lib/principal";
import { getWorkspaceId } from "@/lib/workspace";

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

export async function fetchWithRetry(
  url: string,
  options: RetryOptions = {},
): Promise<Response> {
  const { maxRetries = 2, ...fetchOptions } = options;
  let lastError: Error | null = null;

  for (let attempt = 0; attempt <= maxRetries; attempt++) {
    try {
      const response = await fetchWithTimeout(url, fetchOptions);

      // Retry on 429 Too Many Requests
      if (response.status === 429) {
        lastError = new Error(`HTTP 429`);
        if (attempt < maxRetries) {
          const retryAfter = response.headers.get("retry-after");
          const waitMs = retryAfter
            ? (parseInt(retryAfter, 10) || 2) * 1000
            : Math.min(1000 * 2 ** attempt, 8000);
          await new Promise((r) => setTimeout(r, waitMs));
        }
        continue;
      }

      // Don't retry other client errors (4xx)
      if (response.ok || (response.status >= 400 && response.status < 500)) {
        return response;
      }
      // Server error — retry
      lastError = new Error(`HTTP ${response.status}`);
    } catch (err) {
      if (err instanceof DOMException && err.name === "AbortError") {
        lastError = new Error("Request timed out");
      } else {
        lastError = err as Error;
      }
    }

    if (attempt < maxRetries) {
      await new Promise((r) => setTimeout(r, 100 * Math.pow(2, attempt)));
    }
  }

  throw lastError ?? new Error("Request failed");
}

// ---------------------------------------------------------------------------
// ApiError
// ---------------------------------------------------------------------------

export class ApiError extends Error {
  type?: string;
  details?: unknown;

  constructor(message: string, options?: { type?: string; details?: unknown }) {
    super(message);
    this.name = "ApiError";
    this.type = options?.type;
    this.details = options?.details;
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
        type: body.error?.type,
        details: body.error?.details,
      },
    );
  }

  return parseResponse(res);
}

export async function request<T>(path: string, init?: RetryOptions): Promise<T> {
  return requestInternal(path, init, (res) =>
    res.status === 204 ? Promise.resolve(undefined as T) : res.json(),
  );
}

export async function requestText(path: string, init?: RetryOptions): Promise<string> {
  return requestInternal(path, init, (res) => res.text());
}
