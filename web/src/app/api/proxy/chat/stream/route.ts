import { forwardProtectedRequest } from "@/lib/server/api-proxy";

export const runtime = "nodejs";
export const maxDuration = 120;

export async function POST(request: Request) {
  return forwardProtectedRequest(request, "/chat/stream");
}
