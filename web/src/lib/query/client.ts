import { QueryCache, QueryClient, type DefaultOptions } from "@tanstack/react-query";
import { ApiError } from "@/lib/api";
import { authKeys } from "@/hooks/use-auth";

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
      if (error instanceof ApiError && error.isClientError()) {
        return false;
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
 * Global error sink — when ANY query throws an `ApiError(401)`, the user's
 * session has lapsed mid-flight (cookie expired, token revoked, etc.).
 * The `useAuth` query is cached with `staleTime: Infinity` so it never
 * refetches on its own; without invalidation the page stays stuck on
 * its own ErrorState forever. Invalidating the auth cache flips
 * `useAuth` back into `loading` → `unauthenticated`, and `<AuthGuard>`
 * routes to `/login?next=...` automatically — root-cause handling for
 * "session expired mid-session" instead of a generic retry button that
 * can't recover.
 *
 * 403 (forbidden) is intentionally NOT handled here — the user IS
 * authenticated but lacks permission for that specific resource; a
 * page-level inline error message is the right surface.
 */
function handleQueryError(error: unknown, queryClient: QueryClient): void {
  if (error instanceof ApiError && error.status === 401) {
    queryClient.invalidateQueries({ queryKey: authKeys.me() });
  }
}

function makeClient(): QueryClient {
  const client: QueryClient = new QueryClient({
    defaultOptions: queryDefaults,
    queryCache: new QueryCache({
      onError: (error) => handleQueryError(error, client),
    }),
  });
  return client;
}

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
    return makeClient();
  }
  if (!browserQueryClient) {
    browserQueryClient = makeClient();
  }
  return browserQueryClient;
}
