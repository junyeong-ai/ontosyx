import { forwardProtectedRequest } from "@/lib/server/api-proxy";

export const runtime = "nodejs";

export async function POST(
  request: Request,
  { params }: { params: Promise<{ id: string; rev: string }> },
) {
  const { id, rev } = await params;
  return forwardProtectedRequest(
    request,
    `/projects/${id}/revisions/${rev}/restore`,
  );
}
