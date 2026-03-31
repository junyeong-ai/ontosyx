import { forwardProtectedRequest } from "@/lib/server/api-proxy";
export const runtime = "nodejs";
export async function POST(request: Request) { return forwardProtectedRequest(request, "/ontology/export/python"); }
