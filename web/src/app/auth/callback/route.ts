/**
 * GET /auth/callback — Google OAuth callback handler.
 *
 * Verifies the CSRF state parameter, exchanges the authorization code
 * for tokens, then delegates JWT creation to the backend via POST /auth/token.
 * The backend verifies the Google ID token, upserts the user, and returns
 * a platform JWT with role — making the backend the sole JWT issuer.
 */
import { NextResponse, type NextRequest } from "next/server";
import { COOKIE_NAME } from "@/lib/server/auth";

interface GoogleTokenResponse {
  id_token: string;
  access_token: string;
  token_type: string;
  expires_in: number;
  refresh_token?: string;
}

interface BackendTokenResponse {
  token: string;
  user: {
    id: string;
    email: string;
    name?: string;
    picture?: string;
    role: string;
  };
}

const BACKEND =
  process.env.ONTOSYX_API_URL ?? "http://localhost:3001/api";

export async function GET(request: NextRequest) {
  const baseUrl = process.env.NEXTAUTH_URL ?? "http://localhost:3000";
  const code = request.nextUrl.searchParams.get("code");
  const state = request.nextUrl.searchParams.get("state");
  const error = request.nextUrl.searchParams.get("error");

  if (error) {
    return NextResponse.redirect(`${baseUrl}/login?error=${encodeURIComponent(error)}`);
  }

  // Verify CSRF state
  const storedState = request.cookies.get("oauth_state")?.value;
  if (!state || !storedState || state !== storedState) {
    return NextResponse.redirect(`${baseUrl}/login?error=invalid_state`);
  }

  if (!code) {
    return NextResponse.redirect(`${baseUrl}/login?error=missing_code`);
  }

  const clientId = process.env.GOOGLE_CLIENT_ID;
  const clientSecret = process.env.GOOGLE_CLIENT_SECRET;

  if (!clientId || !clientSecret) {
    return NextResponse.redirect(`${baseUrl}/login?error=not_configured`);
  }

  // Step 1: Exchange authorization code for tokens with Google
  const tokenResponse = await fetch("https://oauth2.googleapis.com/token", {
    method: "POST",
    headers: { "Content-Type": "application/x-www-form-urlencoded" },
    body: new URLSearchParams({
      code,
      client_id: clientId,
      client_secret: clientSecret,
      redirect_uri: `${baseUrl}/auth/callback`,
      grant_type: "authorization_code",
    }),
  });

  if (!tokenResponse.ok) {
    return NextResponse.redirect(`${baseUrl}/login?error=token_exchange_failed`);
  }

  const tokens = (await tokenResponse.json()) as GoogleTokenResponse;

  // Step 2: Delegate to backend — it verifies, upserts user, and issues JWT
  const backendResponse = await fetch(`${BACKEND}/auth/token`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      id_token: tokens.id_token,
      provider: "google",
    }),
  });

  if (!backendResponse.ok) {
    const body = await backendResponse.json().catch(() => ({}));
    const msg = body.error?.message ?? "backend_auth_failed";
    return NextResponse.redirect(`${baseUrl}/login?error=${encodeURIComponent(msg)}`);
  }

  const { token } = (await backendResponse.json()) as BackendTokenResponse;

  // Step 3: Store backend-issued JWT in HTTP-only cookie
  const response = NextResponse.redirect(baseUrl);
  response.cookies.set(COOKIE_NAME, token, {
    httpOnly: true,
    secure: process.env.NODE_ENV === "production",
    sameSite: "lax",
    path: "/",
    maxAge: 7 * 24 * 60 * 60, // 7 days
  });
  response.cookies.set("oauth_state", "", {
    path: "/auth/callback",
    maxAge: 0,
  });

  return response;
}
