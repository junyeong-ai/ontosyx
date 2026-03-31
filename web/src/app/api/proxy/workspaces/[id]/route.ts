import { forwardProtectedRequest } from "@/lib/server/api-proxy";
export const runtime = "nodejs";
export async function GET(request: Request, { params }: { params: Promise<{ id: string }> }) {
  const { id } = await params; return forwardProtectedRequest(request, `/workspaces/${id}`);
}
export async function PATCH(request: Request, { params }: { params: Promise<{ id: string }> }) {
  const { id } = await params; return forwardProtectedRequest(request, `/workspaces/${id}`);
}
export async function DELETE(request: Request, { params }: { params: Promise<{ id: string }> }) {
  const { id } = await params; return forwardProtectedRequest(request, `/workspaces/${id}`);
}
