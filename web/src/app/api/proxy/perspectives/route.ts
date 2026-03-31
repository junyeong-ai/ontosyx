import { forwardProtectedRequest } from "@/lib/server/api-proxy";

export const runtime = "nodejs";

export async function PUT(request: Request) {
  return forwardProtectedRequest(request, "/perspectives");
}
