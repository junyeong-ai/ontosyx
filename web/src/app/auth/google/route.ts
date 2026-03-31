/**
 * GET /auth/google — Redirect to Google OAuth authorization URL.
 *
 * Generates a random `state` parameter stored in a short-lived cookie
 * to prevent CSRF attacks on the callback.
 */
import { NextResponse } from "next/server";

export async function GET() {
  const clientId = process.env.GOOGLE_CLIENT_ID;
  const baseUrl = process.env.NEXTAUTH_URL ?? "http://localhost:3000";

  if (!clientId) {
    return NextResponse.json(
      { error: { type: "not_configured", message: "Google OAuth is not configured" } },
      { status: 503 },
    );
  }

  const state = crypto.randomUUID();

  const params = new URLSearchParams({
    client_id: clientId,
    redirect_uri: `${baseUrl}/auth/callback`,
    response_type: "code",
    scope: "openid email profile",
    access_type: "offline",
    prompt: "consent",
    state,
  });

  const response = NextResponse.redirect(
    `https://accounts.google.com/o/oauth2/v2/auth?${params}`,
  );

  // Store state in a short-lived HTTP-only cookie for callback verification
  response.cookies.set("oauth_state", state, {
    httpOnly: true,
    secure: process.env.NODE_ENV === "production",
    sameSite: "lax",
    path: "/auth/callback",
    maxAge: 600, // 10 minutes
  });

  return response;
}
