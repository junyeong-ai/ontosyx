// ---------------------------------------------------------------------------
// Workspace types — multi-tenant organization model
// ---------------------------------------------------------------------------

export interface WorkspaceSummary {
  id: string;
  name: string;
  slug: string;
  owner_id: string;
  role: string;
  member_count: number;
  created_at: string;
}

export interface Workspace {
  id: string;
  name: string;
  slug: string;
  owner_id: string;
  settings: Record<string, unknown>;
  /** Canonical BCP 47 tag used as the workspace's primary UI/LLM locale. */
  primary_locale: string;
  /** Ordered fallback chain — non-empty list of BCP 47 tags. */
  locale_fallback: string[];
  created_at: string;
}

export interface UpdateWorkspaceLocaleRequest {
  primary_locale: string;
  locale_fallback: string[];
}

export interface WorkspaceMember {
  workspace_id: string;
  user_id: string;
  role: string;
  joined_at: string;
  email?: string;
  name?: string;
  picture?: string;
}

export interface CreateWorkspaceRequest {
  name: string;
  slug: string;
}

export interface UpdateWorkspaceRequest {
  name: string;
  settings?: Record<string, unknown>;
}

export interface AddMemberRequest {
  user_id: string;
  role?: string;
}
