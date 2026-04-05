import { forwardProtectedRequest } from "@/lib/server/api-proxy";
export const runtime = "nodejs";
export async function PATCH(request: Request, { params }: { params: Promise<{ id: string }> }) {
  const { id } = await params; return forwardProtectedRequest(request, `/notifications/channels/${id}`);
}
export async function DELETE(request: Request, { params }: { params: Promise<{ id: string }> }) {
  const { id } = await params; return forwardProtectedRequest(request, `/notifications/channels/${id}`);
}
