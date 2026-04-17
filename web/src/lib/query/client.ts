import { QueryClient, type DefaultOptions } from "@tanstack/react-query";
import { ApiError } from "@/lib/api";

/**
 * Shared default options for every query/mutation in the app.
 *
 * Why: Centralising defaults avoids scattered per-hook config. Teams that
 * need stricter staleness or retries override locally; the baseline below
 * matches "typical knowledge-graph read" traffic — not too chatty, not stale.
 */
export const queryDefaults: DefaultOptions = {
  queries: {
    // Why: 30s keeps most reads snappy on tab-switch without pounding the API.
    // Mutations bust caches via invalidateQueries so explicit freshness is
    // driven by writes, not wall-clock refetches.
    staleTime: 30_000,

    // Why: 5 minutes of gcTime (previously `cacheTime`) — long enough to
    // survive a quick navigation round-trip, short enough to avoid unbounded
    // memory growth for workspaces that touch many pages.
    gcTime: 5 * 60_000,

    // Why: Most screens (settings, lists) don't need to refetch just because
    // the user blurred and returned. Real-time surfaces (SSE streams, graph
    // widgets) already push their own updates. Per-hook can opt back in with
    // `refetchOnWindowFocus: true` when it matters.
    refetchOnWindowFocus: false,

    // Why: Do not retry 4xx — they're deterministic and waste API budget.
    // Retry other errors twice with exponential backoff (TanStack default
    // delay is sufficient).
    retry: (failureCount, error) => {
      if (error instanceof ApiError) {
        const message = error.message ?? "";
        // Heuristic: ApiError with a 4xx-looking message is not retryable.
        if (/\b4\d{2}\b/.test(message) || error.type === "not_found" || error.type === "forbidden") {
          return false;
        }
      }
      return failureCount < 2;
    },
  },
  mutations: {
    // Why: Mutations are rarely idempotent in this app (create/delete),
    // so retrying risks duplicate writes. Callers can override for specific
    // idempotent cases (e.g. status-only updates).
    retry: false,
  },
};

/**
 * Singleton QueryClient for the browser.
 *
 * Why singleton: Next.js App Router mounts providers once per client boot;
 * sharing the instance across components preserves the cache on route
 * navigation (without reloading). We lazily construct on first access so SSR
 * bundles don't pull in client-only state.
 */
let browserQueryClient: QueryClient | undefined;

export function getQueryClient(): QueryClient {
  if (typeof window === "undefined") {
    // Server-side: always a fresh client — the cache is not shared between
    // requests (would leak workspace data between users).
    return new QueryClient({ defaultOptions: queryDefaults });
  }
  if (!browserQueryClient) {
    browserQueryClient = new QueryClient({ defaultOptions: queryDefaults });
  }
  return browserQueryClient;
}
