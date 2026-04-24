/**
 * Ontology detail proxy — `GET /api/proxy/ontologies/{id}`.
 *
 * Sub-resources (edits, map-summary, axis-items, …) live under
 * the sibling `[...rest]` catch-all route. This file handles only
 * the exact-match detail path so Next.js's dynamic-segment router
 * has a clear match for the no-sub-path case.
 */
import { forwardProtectedRequest } from "@/lib/server/api-proxy";

export const runtime = "nodejs";

type Params = { params: Promise<{ id: string }> };

export async function GET(request: Request, { params }: Params) {
  const { id } = await params;
  return forwardProtectedRequest(
    request,
    `/ontologies/${encodeURIComponent(id)}`,
  );
}
