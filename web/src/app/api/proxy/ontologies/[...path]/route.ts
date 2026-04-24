/**
 * Catch-all proxy for every `/api/proxy/ontologies/…` sub-path.
 *
 * The ontology surface is uniformly RESTful under `/ontologies/*`
 * on the backend — both collection-level verbs (export, import,
 * normalize, suggestions, adopt-graph, type-candidates) and
 * resource-scoped sub-resources (`{id}`, `{id}/edits`,
 * `{id}/map-summary`, `{id}/axis-items`, `{id}/cross-refs`,
 * `{id}/audit`, `{id}/reindex`, `{id}/verifications/{elem}`,
 * `{id}/glossary/suggest-bindings`, …).
 *
 * Every single path is pure forwarding with identical shape, so one
 * catch-all replaces what would otherwise be ~25 boilerplate files
 * and automatically picks up new backend sub-resources with no FE
 * proxy work. The root `ontologies/route.ts` still handles the bare
 * `/api/proxy/ontologies` (list GET + create POST) because Next.js
 * resolves the static route before a catch-all.
 *
 * Query string + request body are preserved by
 * `forwardProtectedRequest`; upstream content-type flows through so
 * the `text/plain` exports (cypher / mermaid / owl / …) and the SSE
 * stream paths work unchanged.
 */
import { forwardProtectedRequest } from "@/lib/server/api-proxy";

export const runtime = "nodejs";

type Params = { params: Promise<{ path: string[] }> };

async function target({ params }: Params): Promise<string> {
  const { path } = await params;
  return `/ontologies/${path.join("/")}`;
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
