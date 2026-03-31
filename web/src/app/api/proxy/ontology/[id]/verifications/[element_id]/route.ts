import { forwardProtectedRequest } from "@/lib/server/api-proxy";

export const runtime = "nodejs";

export async function DELETE(
  request: Request,
  { params }: { params: Promise<{ id: string; element_id: string }> },
) {
  const { id, element_id } = await params;
  return forwardProtectedRequest(
    request,
    `/ontology/${id}/verifications/${element_id}`,
  );
}
