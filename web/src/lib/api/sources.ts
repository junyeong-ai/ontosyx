import { request } from "./client";
import type { components } from "@/types/api.generated";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export type TestConnectionRequest = components["schemas"]["TestConnectionRequest"];
export type TestConnectionResponse = components["schemas"]["TestConnectionResponse"];

// ---------------------------------------------------------------------------
// Sources
// ---------------------------------------------------------------------------

export async function testSourceConnection(
  req: TestConnectionRequest,
): Promise<TestConnectionResponse> {
  return request<TestConnectionResponse>("/sources/test-connection", {
    method: "POST",
    body: JSON.stringify(req),
  });
}
