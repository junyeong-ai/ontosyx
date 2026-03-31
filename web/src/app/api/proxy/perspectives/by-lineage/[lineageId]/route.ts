import { forwardProtectedRequest } from "@/lib/server/api-proxy";

export const runtime = "nodejs";

export async function GET(
  request: Request,
  { params }: { params: Promise<{ lineageId: string }> },
) {
  const { lineageId } = await params;
  return forwardProtectedRequest(request, `/perspectives/by-lineage/${lineageId}`);
}
