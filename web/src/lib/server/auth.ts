/**
 * Server-side auth utilities for JWT session management.
 *
 * JWTs are issued exclusively by the Rust backend (POST /auth/token).
 * This module only reads and verifies them from the HTTP-only cookie.
 * When GOOGLE_CLIENT_ID is not configured, auth is disabled (dev mode).
 */
import { jwtVerify, type JWTPayload } from "jose";
import { cookies } from "next/headers";

export const COOKIE_NAME = "ontosyx_session";
const ISSUER = "ontosyx";

/** Subset of AuthClaims needed for frontend display/routing. */
export interface SessionUser {
  sub: string;
  email: string;
  name: string;
  role: string;
}

/** JWT payload structure matching the Rust AuthClaims. */
interface AuthClaims extends JWTPayload {
  sub: string;
  email: string;
  name?: string;
  role: string;
  iss: string;
}

/** Returns true when Google OIDC is configured. */
export function isAuthEnabled(): boolean {
  return !!(
    process.env.GOOGLE_CLIENT_ID &&
    process.env.GOOGLE_CLIENT_SECRET &&
    process.env.AUTH_JWT_SECRET
  );
}

function getSecret(): Uint8Array {
  const secret = process.env.AUTH_JWT_SECRET;
  if (!secret) {
    throw new Error("AUTH_JWT_SECRET is not configured");
  }
  return new TextEncoder().encode(secret);
}

/** Verify and decode the backend-issued JWT. Returns null if invalid/expired. */
async function verifySessionJwt(
  token: string,
): Promise<SessionUser | null> {
  try {
    const { payload } = await jwtVerify<AuthClaims>(token, getSecret(), {
      issuer: ISSUER,
    });
    if (!payload.sub || !payload.email) return null;
    return {
      sub: payload.sub,
      email: payload.email,
      name: payload.name ?? "",
      role: payload.role ?? "designer",
    };
  } catch {
    return null;
  }
}

/** Read the session user from the cookie. Returns null if not authenticated. */
export async function getSessionUser(): Promise<SessionUser | null> {
  if (!isAuthEnabled()) return null;

  const cookieStore = await cookies();
  const token = cookieStore.get(COOKIE_NAME)?.value;
  if (!token) return null;
  return verifySessionJwt(token);
}
