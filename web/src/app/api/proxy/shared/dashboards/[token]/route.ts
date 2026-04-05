import { NextResponse } from "next/server";

export const runtime = "nodejs";

const BACKEND_URL = process.env.ONTOSYX_API_URL || "http://localhost:3101/api";

export async function GET(
  _request: Request,
  { params }: { params: Promise<{ token: string }> },
) {
  const { token } = await params;
  const res = await fetch(`${BACKEND_URL}/shared/dashboards/${token}`, {
    headers: { "Content-Type": "application/json" },
    cache: "no-store",
  });
  const body = await res.text();
  return new NextResponse(body, {
    status: res.status,
    headers: { "Content-Type": "application/json" },
  });
}
