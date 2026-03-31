import { forwardPublicRequest } from "@/lib/server/api-proxy";

export const runtime = "nodejs";

export async function GET() {
  return forwardPublicRequest("/health");
}
