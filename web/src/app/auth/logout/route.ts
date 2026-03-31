/**
 * POST /auth/logout — Clear the session cookie and redirect to login.
 */
import { NextResponse } from "next/server";
import { COOKIE_NAME } from "@/lib/server/auth";

export async function POST() {
  const baseUrl = process.env.NEXTAUTH_URL ?? "http://localhost:3000";
  const response = NextResponse.redirect(baseUrl + "/login");
  response.cookies.set(COOKIE_NAME, "", {
    httpOnly: true,
    secure: process.env.NODE_ENV === "production",
    sameSite: "lax",
    path: "/",
    maxAge: 0,
  });
  return response;
}
