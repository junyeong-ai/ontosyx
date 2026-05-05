import { describe, it, expect, vi, beforeEach } from "vitest";
import { renderHook, waitFor } from "@testing-library/react";
import { QueryClientProvider, useQuery } from "@tanstack/react-query";
import type { ReactNode } from "react";

import { ApiError } from "@/lib/api";
import { authKeys } from "@/hooks/use-auth";
import { getQueryClient } from "@/lib/query/client";

function wrap(client: ReturnType<typeof getQueryClient>) {
  function Wrapper({ children }: { children: ReactNode }) {
    return <QueryClientProvider client={client}>{children}</QueryClientProvider>;
  }
  return Wrapper;
}

describe("global QueryCache 401 handler", () => {
  beforeEach(() => {
    // Reset the singleton so each test gets a clean cache + handler.
    // The handler is wired in the QueryCache constructor; we can't
    // re-run that without creating a new client.
    // `getQueryClient` reads `typeof window !== "undefined"` and caches
    // on the module — patch by clearing.
    // (We import via a fresh module each time would be cleaner; for
    //  now, invalidate auth state at start of each test.)
  });

  it("invalidates the auth cache when a query throws ApiError(401)", async () => {
    const client = getQueryClient();
    // Seed the auth cache so we can observe invalidation.
    client.setQueryData(authKeys.me(), { sub: "u1", email: "x@y", name: "U", role: "admin", auth_enabled: true });
    expect(client.getQueryState(authKeys.me())?.dataUpdatedAt).toBeGreaterThan(0);

    const failing401 = vi.fn().mockRejectedValue(
      new ApiError({ status: 401 }),
    );

    const { result } = renderHook(
      () =>
        useQuery({
          queryKey: ["test", "401"],
          queryFn: failing401,
          retry: false,
        }),
      { wrapper: wrap(client) },
    );

    await waitFor(() => expect(result.current.isError).toBe(true));

    // After the error, the auth query state should be marked as
    // invalidated — TanStack sets `isInvalidated: true` on the entry.
    await waitFor(() => {
      const state = client.getQueryState(authKeys.me());
      expect(state?.isInvalidated).toBe(true);
    });
  });

  it("does NOT invalidate auth cache for 403 (forbidden but authenticated)", async () => {
    const client = getQueryClient();
    client.setQueryData(authKeys.me(), { sub: "u1", email: "x@y", name: "U", role: "viewer", auth_enabled: true });
    // Mark non-invalidated to start.
    client.invalidateQueries({ queryKey: authKeys.me() });
    client.setQueryData(authKeys.me(), { sub: "u1", email: "x@y", name: "U", role: "viewer", auth_enabled: true });

    const failing403 = vi.fn().mockRejectedValue(
      new ApiError({ status: 403 }),
    );

    const { result } = renderHook(
      () =>
        useQuery({
          queryKey: ["test", "403"],
          queryFn: failing403,
          retry: false,
        }),
      { wrapper: wrap(client) },
    );

    await waitFor(() => expect(result.current.isError).toBe(true));

    // 403 is a permission issue, not a session issue — auth cache
    // should NOT be touched.
    const state = client.getQueryState(authKeys.me());
    expect(state?.isInvalidated).toBe(false);
  });

  it("does NOT invalidate auth cache for 5xx (server error, not session)", async () => {
    const client = getQueryClient();
    client.setQueryData(authKeys.me(), { sub: "u1", email: "x@y", name: "U", role: "admin", auth_enabled: true });
    client.invalidateQueries({ queryKey: authKeys.me() });
    client.setQueryData(authKeys.me(), { sub: "u1", email: "x@y", name: "U", role: "admin", auth_enabled: true });

    const failing500 = vi.fn().mockRejectedValue(
      new ApiError({ status: 500 }),
    );

    const { result } = renderHook(
      () =>
        useQuery({
          queryKey: ["test", "500"],
          queryFn: failing500,
          retry: false,
        }),
      { wrapper: wrap(client) },
    );

    await waitFor(() => expect(result.current.isError).toBe(true));

    const state = client.getQueryState(authKeys.me());
    expect(state?.isInvalidated).toBe(false);
  });
});
