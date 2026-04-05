import { forwardProtectedRequest } from "@/lib/server/api-proxy";
export const runtime = "nodejs";
export async function GET(request: Request) { return forwardProtectedRequest(request, "/notifications/channels"); }
export async function POST(request: Request) { return forwardProtectedRequest(request, "/notifications/channels"); }
