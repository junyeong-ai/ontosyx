/**
 * Server-side API proxy for Next.js API routes (BFF pattern).
 *
 * Security boundary:
 * - When auth is enabled, the session JWT cookie is forwarded as an
 *   Authorization: Bearer header. The backend verifies the JWT directly
 *   with the shared secret — no claim extraction or header rewriting here.
 * - When auth is disabled (dev mode), the API key is injected server-side
 *   and x-principal-id is forwarded from the browser for user scoping.
 * - No other request headers are forwarded to prevent header injection.
 */
import { cookies } from "next/headers";
import { isAuthEnabled, COOKIE_NAME } from "./auth";

const BACKEND =
  process.env.ONTOSYX_API_URL ?? "http://localhost:3001/api";

const API_KEY = process.env.OX_API_KEY;

/**
 * Forward a request to a public backend endpoint (no credentials).
 * Used for /health, /config/ui — avoids cross-origin issues.
 */
export async function forwardPublicRequest(
  backendPath: string,
): Promise<Response> {
  const upstream = await fetch(`${BACKEND}${backendPath}`, {
    method: "GET",
    headers: { "content-type": "application/json" },
    cache: "no-store",
  });

  const body = await upstream.text();
  return new Response(body, {
    status: upstream.status,
    headers: { "content-type": upstream.headers.get("content-type") ?? "application/json" },
  });
}

/**
 * Forward a request to a protected backend endpoint.
 *
 * Auth enabled:  session JWT → Authorization: Bearer (backend verifies)
 * Auth disabled: API key → x-api-key + x-principal-id (dev mode)
 */
export async function forwardProtectedRequest(
  request: Request,
  backendPath: string,
): Promise<Response> {
  const headers = new Headers();

  const contentType = request.headers.get("content-type");
  if (contentType) {
    headers.set("content-type", contentType);
  }

  if (isAuthEnabled()) {
    // Forward session JWT directly — backend verifies with shared secret
    const cookieStore = await cookies();
    const token = cookieStore.get(COOKIE_NAME)?.value;
    if (token) {
      headers.set("authorization", `Bearer ${token}`);
    } else if (API_KEY) {
      // Authenticated context but no session (e.g. server-side call)
      headers.set("x-api-key", API_KEY);
    }
  } else {
    // Dev mode: API key + browser-supplied principal ID
    if (!API_KEY) {
      return Response.json(
        { error: { type: "service_unavailable", message: "API key not configured. Set OX_API_KEY." } },
        { status: 503 },
      );
    }
    headers.set("x-api-key", API_KEY);
    const principalId = request.headers.get("x-principal-id");
    if (principalId) {
      headers.set("x-principal-id", principalId);
    }
  }

  // Forward workspace context header
  const workspaceId = request.headers.get("x-workspace-id");
  if (workspaceId) {
    headers.set("x-workspace-id", workspaceId);
  }

  // Forward query string
  const url = new URL(request.url);
  const upstreamPath = url.search
    ? `${backendPath}${url.search}`
    : backendPath;

  const upstream = await fetch(`${BACKEND}${upstreamPath}`, {
    method: request.method,
    headers,
    body: request.method === "GET" || request.method === "HEAD"
      ? undefined
      : await request.text(),
    cache: "no-store",
  });

  if (upstream.status === 204) {
    return new Response(null, { status: 204 });
  }

  // Stream SSE responses through without buffering
  const responseContentType = upstream.headers.get("content-type") ?? "application/json";
  if (responseContentType.includes("text/event-stream")) {
    return new Response(upstream.body, {
      status: upstream.status,
      headers: {
        "content-type": "text/event-stream",
        "cache-control": "no-cache",
        "connection": "keep-alive",
      },
    });
  }

  const body = await upstream.text();
  return new Response(body, {
    status: upstream.status,
    headers: { "content-type": responseContentType },
  });
}
