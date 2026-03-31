/**
 * GET /auth/me — Return current user info from the JWT cookie.
 *
 * Returns the authenticated user or 401 if not authenticated.
 * When auth is disabled (dev mode), returns a synthetic dev user.
 */
import { NextResponse } from "next/server";
import { getSessionUser, isAuthEnabled } from "@/lib/server/auth";

export async function GET() {
  if (!isAuthEnabled()) {
    return NextResponse.json({
      sub: "dev",
      email: "dev@localhost",
      name: "Developer",
      role: "admin",
      auth_enabled: false,
    });
  }

  const user = await getSessionUser();
  if (!user) {
    return NextResponse.json(
      { error: { type: "unauthorized", message: "Not authenticated" } },
      { status: 401 },
    );
  }

  return NextResponse.json({ ...user, auth_enabled: true });
}
