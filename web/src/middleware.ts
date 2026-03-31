/**
 * Next.js middleware for auth gating.
 *
 * When Google OIDC is configured (GOOGLE_CLIENT_ID env var is set),
 * redirects unauthenticated requests to the login page.
 * When not configured, all requests pass through (dev mode).
 */
import { NextResponse } from "next/server";
import type { NextRequest } from "next/server";

const COOKIE_NAME = "ontosyx_session";

export function middleware(request: NextRequest) {
  // Skip auth entirely when Google OIDC is not configured (dev mode)
  const authEnabled = !!process.env.GOOGLE_CLIENT_ID;
  if (!authEnabled) {
    return NextResponse.next();
  }

  const token = request.cookies.get(COOKIE_NAME)?.value;
  const { pathname } = request.nextUrl;

  // Allow auth routes, login page, and public assets
  const isPublicPath =
    pathname.startsWith("/auth/") ||
    pathname === "/login" ||
    pathname.startsWith("/api/") ||
    pathname.startsWith("/_next/") ||
    pathname === "/favicon.ico";

  if (!token && !isPublicPath) {
    return NextResponse.redirect(new URL("/login", request.url));
  }

  // If authenticated user visits /login, redirect to home
  if (token && pathname === "/login") {
    return NextResponse.redirect(new URL("/", request.url));
  }

  return NextResponse.next();
}

export const config = {
  matcher: ["/((?!_next/static|_next/image|favicon.ico).*)"],
};
