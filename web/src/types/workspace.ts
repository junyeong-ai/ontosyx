// ---------------------------------------------------------------------------
// Workspace types — multi-tenant organization model.
//
// Wire shapes flow through the OpenAPI spec; thin FE aliases below
// keep call-site imports stable while staying in lockstep with the
// backend. Adding a field to the Rust DTO surfaces here through the
// next `gen-openapi-types.sh` run.
// ---------------------------------------------------------------------------

import type { components } from "@/types/api.generated";

export type WorkspaceSummary = components["schemas"]["WorkspaceSummaryResponse"];
export type Workspace = components["schemas"]["WorkspaceResponse"];
export type WorkspaceMember = components["schemas"]["MemberResponse"];

export type CreateWorkspaceRequest =
  components["schemas"]["CreateWorkspaceRequest"];
export type UpdateWorkspaceRequest =
  components["schemas"]["UpdateWorkspaceRequest"];
export type UpdateWorkspaceLocaleRequest =
  components["schemas"]["UpdateWorkspaceLocaleRequest"];
export type AddMemberRequest = components["schemas"]["AddMemberRequest"];
