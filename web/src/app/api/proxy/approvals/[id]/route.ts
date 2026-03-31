import { forwardProtectedRequest } from "@/lib/server/api-proxy";
export const runtime = "nodejs";
export async function GET(request: Request, { params }: { params: Promise<{ id: string }> }) {
  const { id } = await params; return forwardProtectedRequest(request, `/approvals/${id}`);
}
