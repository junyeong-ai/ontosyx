import { forwardProtectedRequest } from "@/lib/server/api-proxy";

export const runtime = "nodejs";

export async function PATCH(
  request: Request,
  { params }: { params: Promise<{ id: string; widget_id: string }> },
) {
  const { id, widget_id } = await params;
  return forwardProtectedRequest(
    request,
    `/dashboards/${id}/widgets/${widget_id}`,
  );
}

export async function DELETE(
  request: Request,
  { params }: { params: Promise<{ id: string; widget_id: string }> },
) {
  const { id, widget_id } = await params;
  return forwardProtectedRequest(
    request,
    `/dashboards/${id}/widgets/${widget_id}`,
  );
}
