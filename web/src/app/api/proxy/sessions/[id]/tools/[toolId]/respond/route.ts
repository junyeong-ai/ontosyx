import { forwardProtectedRequest } from "@/lib/server/api-proxy";

export const runtime = "nodejs";

export async function POST(
  request: Request,
  { params }: { params: Promise<{ id: string; toolId: string }> },
) {
  const { id, toolId } = await params;
  return forwardProtectedRequest(
    request,
    `/sessions/${id}/tools/${toolId}/respond`,
  );
}
