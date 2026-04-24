import { forwardProtectedRequest } from "@/lib/server/api-proxy";

export const runtime = "nodejs";

export async function GET(request: Request) {
  return forwardProtectedRequest(request, "/ontologies");
}

// `POST /api/ontologies` is the unified creation endpoint —
// creates an identity + applies a batch of `OntologyEditOp`s as v1.
// The bootstrap wizard and the admin UI both land here.
export async function POST(request: Request) {
  return forwardProtectedRequest(request, "/ontologies");
}
