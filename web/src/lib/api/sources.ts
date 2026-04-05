import { request } from "./client";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface TestConnectionRequest {
  source_type: string;
  connection_string?: string;
  schema_name?: string;
}

export interface TestConnectionResponse {
  success: boolean;
  table_count?: number;
  tables?: string[];
  error?: string;
  error_type?: string;
}

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
