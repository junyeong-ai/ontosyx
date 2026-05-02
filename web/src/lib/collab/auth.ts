// Token provider for the collaboration WebSocket. The platform's
// session cookie is `httpOnly`, so the WS auth frame needs an
// explicit JWT minted via `GET /auth/ws-token`. The cache below
// reuses one mint across the typical connect → reconnect window —
// the endpoint's TTL is short (120s on the server today) so a stale
// token can't outlive a logout by more than a few seconds.

import { request } from "@/lib/api/client";
import type { components } from "@/types/api.generated";

type WsTokenResponse = components["schemas"]["WebSocketTokenResponse"];

interface CachedToken {
  token: string;
  expiresAtMs: number;
}

let cached: CachedToken | null = null;

/**
 * Margin (ms) before the token's actual expiry at which we treat
 * the cache as stale and re-mint. Covers clock skew + the time it
 * takes the WS handshake to reach the server.
 */
const EXPIRY_MARGIN_MS = 5_000;

/**
 * Fetch a fresh WS token, cached until shortly before expiry.
 * `CollaborationClient.getToken` calls this on every (re)connect.
 */
export async function fetchWsToken(): Promise<string> {
  const now = Date.now();
  if (cached && now < cached.expiresAtMs - EXPIRY_MARGIN_MS) {
    return cached.token;
  }
  const r = await request<WsTokenResponse>("/auth/ws-token");
  cached = {
    token: r.token,
    expiresAtMs: new Date(r.expires_at).getTime(),
  };
  return r.token;
}

/**
 * Drop the cached token. Call on sign-out so a stale mint can't
 * be reused for the next session.
 */
export function clearWsTokenCache(): void {
  cached = null;
}
