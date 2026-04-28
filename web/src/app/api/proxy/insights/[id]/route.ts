import { forwardProtectedRequest } from "@/lib/server/api-proxy";

export const runtime = "nodejs";

interface Params {
  params: Promise<{ id: string }>;
}

export async function GET(request: Request, { params }: Params) {
  const { id } = await params;
  return forwardProtectedRequest(request, `/insights/${encodeURIComponent(id)}`);
}

export async function PUT(request: Request, { params }: Params) {
  const { id } = await params;
  return forwardProtectedRequest(request, `/insights/${encodeURIComponent(id)}`);
}

export async function DELETE(request: Request, { params }: Params) {
  const { id } = await params;
  return forwardProtectedRequest(request, `/insights/${encodeURIComponent(id)}`);
}
