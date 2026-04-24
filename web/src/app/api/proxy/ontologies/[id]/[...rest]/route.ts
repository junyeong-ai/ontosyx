/**
 * Catch-all proxy for every sub-resource under
 * `/api/proxy/ontologies/{id}/…`.
 *
 * Every ontology sub-path (edits, map-summary, axis-items,
 * cross-refs, verifications, glossary bindings, enrich, reindex,
 * audit, etc.) is pure boilerplate forwarding with the same
 * shape. A single catch-all saves ~10+ identical files and means
 * new backend sub-resources need no FE proxy work — the handler
 * picks them up by path segment.
 *
 * The `/projects/[id]/*` tree still uses per-endpoint files (see
 * `api/proxy/projects/[id]/`); that tree predates this pattern
 * and stays explicit on purpose. For the ontology tree — which
 * gained five new sub-resources in the v1 reset and is expected
 * to keep growing — the catch-all scales better.
 *
 * Query string is preserved by `forwardProtectedRequest` (reads
 * `new URL(request.url).search`).
 */
import { forwardProtectedRequest } from "@/lib/server/api-proxy";

export const runtime = "nodejs";

type Params = { params: Promise<{ id: string; rest: string[] }> };

async function target({ params }: Params): Promise<string> {
  const { id, rest } = await params;
  // `rest` is the path segment array after `[id]/`. Re-joining with
  // `/` yields the full sub-path the backend expects — `edits`,
  // `map-summary`, `glossary/suggest-bindings`, etc.
  const suffix = rest.join("/");
  return `/ontologies/${encodeURIComponent(id)}/${suffix}`;
}

export async function GET(request: Request, ctx: Params) {
  return forwardProtectedRequest(request, await target(ctx));
}

export async function POST(request: Request, ctx: Params) {
  return forwardProtectedRequest(request, await target(ctx));
}

export async function PATCH(request: Request, ctx: Params) {
  return forwardProtectedRequest(request, await target(ctx));
}

export async function PUT(request: Request, ctx: Params) {
  return forwardProtectedRequest(request, await target(ctx));
}

export async function DELETE(request: Request, ctx: Params) {
  return forwardProtectedRequest(request, await target(ctx));
}
