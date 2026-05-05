import { forwardRequest } from "@/lib/server/api-proxy";

// Single catch-all proxy: every request under `/api/proxy/<segments>`
// is forwarded to `<BACKEND>/<segments>` with the same method, headers
// (auth attached server-side), query string, and body. New backend
// endpoints land instantly — no companion file to add or forget.

export const runtime = "nodejs";

interface Ctx {
  params: Promise<{ path: string[] }>;
}

async function handler(request: Request, ctx: Ctx): Promise<Response> {
  const { path } = await ctx.params;
  const backendPath = `/${path.join("/")}`;
  return forwardRequest(request, backendPath);
}

export const GET = handler;
export const POST = handler;
export const PUT = handler;
export const PATCH = handler;
export const DELETE = handler;
export const HEAD = handler;
