/**
 * Server-side API proxy for Next.js API routes (BFF pattern).
 *
 * Single entry point: `forwardRequest(request, backendPath)`. The
 * caller is the catch-all `/api/proxy/[...path]/route.ts` handler;
 * `backendPath` comes straight from the URL segments. Public vs
 * protected paths are routed by the `PUBLIC_PATHS` set below — every
 * other path receives the auth headers automatically.
 *
 * Security boundary:
 * - When auth is enabled, the session JWT cookie is forwarded as an
 *   Authorization: Bearer header. The backend verifies the JWT directly
 *   with the shared secret — no claim extraction or header rewriting here.
 * - When auth is disabled (dev mode), the API key is injected server-side
 *   and x-principal-id is forwarded from the browser for user scoping.
 * - No other request headers are forwarded to prevent header injection.
 */
import fs from "node:fs";
import { cookies } from "next/headers";
import { isAuthEnabled, COOKIE_NAME } from "./auth";

const BACKEND =
  process.env.ONTOSYX_API_URL ?? "http://localhost:3101/api";

/**
 * Dev API key source of truth. `dev.sh seed` regenerates the key on
 * every backend boot and writes it here; the proxy reads per-request
 * (with an mtime cache) so a re-seed propagates immediately without
 * restarting the Next.js dev server.
 *
 * Path is a two-file contract — keep in sync with the `_creds_file`
 * helper in `scripts/dev.sh`.
 */
const DEV_CREDS_PATH = "/tmp/ontosyx-dev-creds";

let devCredsCache: { apiKey: string | undefined; mtimeMs: number } | null = null;

function readDevApiKey(): string | undefined {
  let mtimeMs: number;
  try {
    mtimeMs = fs.statSync(DEV_CREDS_PATH).mtimeMs;
  } catch {
    devCredsCache = null;
    return undefined;
  }
  if (devCredsCache && devCredsCache.mtimeMs === mtimeMs) {
    return devCredsCache.apiKey;
  }
  try {
    const contents = fs.readFileSync(DEV_CREDS_PATH, "utf8");
    const apiKey = contents.match(/^export OX_API_KEY="([^"]+)"$/m)?.[1];
    devCredsCache = { apiKey, mtimeMs };
    return apiKey;
  } catch {
    devCredsCache = null;
    return undefined;
  }
}

/**
 * API key for the auth-disabled (single-tenant / on-prem / dev) mode.
 *   Production: operator-set `OX_API_KEY` env.
 *   Dev:        the seed-written cred file is the single source.
 */
function getApiKey(): string | undefined {
  return process.env.NODE_ENV === "production"
    ? process.env.OX_API_KEY
    : readDevApiKey();
}

/**
 * Backend paths that don't carry a session — health/config probes
 * the front-end shell needs before any user is signed in. Anything
 * not listed here is treated as protected.
 */
const PUBLIC_PATHS: ReadonlySet<string> = new Set([
  "/health",
  "/healthz",
  "/config/ui",
]);

function isPublic(backendPath: string): boolean {
  return PUBLIC_PATHS.has(backendPath);
}

/**
 * Forward a request to the backend. Auth headers are attached only
 * when the path is not in `PUBLIC_PATHS`.
 */
export async function forwardRequest(
  request: Request,
  backendPath: string,
): Promise<Response> {
  const headers = new Headers();

  const contentType = request.headers.get("content-type");
  if (contentType) {
    headers.set("content-type", contentType);
  }

  if (!isPublic(backendPath)) {
    if (isAuthEnabled()) {
      // JWT-only path. No session cookie ⇒ no headers ⇒ backend 401
      // ⇒ FE redirects to /login. Falling back to the API key here
      // would silently grant admin access to anonymous traffic.
      const cookieStore = await cookies();
      const token = cookieStore.get(COOKIE_NAME)?.value;
      if (token) {
        headers.set("authorization", `Bearer ${token}`);
      }
    } else {
      const apiKey = getApiKey();
      if (!apiKey) {
        return Response.json(
          {
            error: {
              type: "service_unavailable",
              message:
                process.env.NODE_ENV === "production"
                  ? "API key not configured. Set OX_API_KEY."
                  : "Dev credentials missing. Run `./scripts/dev.sh seed`.",
            },
          },
          { status: 503 },
        );
      }
      headers.set("x-api-key", apiKey);
      const principalId = request.headers.get("x-principal-id");
      if (principalId) {
        headers.set("x-principal-id", principalId);
      }
    }

    const workspaceId = request.headers.get("x-workspace-id");
    if (workspaceId) {
      headers.set("x-workspace-id", workspaceId);
    }
  }

  // Forward query string verbatim.
  const url = new URL(request.url);
  const upstreamPath = url.search
    ? `${backendPath}${url.search}`
    : backendPath;

  const method = request.method.toUpperCase();
  const hasBody = method !== "GET" && method !== "HEAD" && method !== "DELETE";

  let upstream: Response;
  try {
    upstream = await fetch(`${BACKEND}${upstreamPath}`, {
      method,
      headers,
      // Stream the body through — never buffer in proxy memory. Required
      // for correctness on multipart / binary uploads (text() would lossily
      // round-trip non-UTF-8 bytes) and for memory bounds on large payloads.
      // `duplex: "half"` is mandated by undici when `body` is a stream;
      // the standard `RequestInit` type doesn't yet include it.
      body: hasBody ? request.body : null,
      cache: "no-store",
      duplex: "half",
    } as RequestInit & { duplex?: "half" });
  } catch (error) {
    console.error("[api-proxy] backend fetch failed:", error);
    return Response.json(
      {
        error: { type: "bad_gateway", message: "Backend unreachable." },
      },
      { status: 502 },
    );
  }

  if (upstream.status === 204) {
    return new Response(null, { status: 204 });
  }

  // Stream the response body through — same reasoning as the request side.
  // SSE just needs no-cache + keep-alive on top of the streaming default.
  const responseContentType = upstream.headers.get("content-type") ?? "application/json";
  const isSSE = responseContentType.includes("text/event-stream");
  return new Response(upstream.body, {
    status: upstream.status,
    headers: isSSE
      ? {
          "content-type": "text/event-stream",
          "cache-control": "no-cache",
          "connection": "keep-alive",
        }
      : { "content-type": responseContentType },
  });
}
