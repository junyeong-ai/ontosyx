import { forwardProtectedRequest } from "@/lib/server/api-proxy";
export const runtime = "nodejs";
export async function GET(request: Request, { params }: { params: Promise<{ label: string }> }) {
  const { label } = await params; return forwardProtectedRequest(request, `/lineage/label/${label}`);
}
