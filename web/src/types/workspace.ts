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
  /** Canonical BCP 47 tag used as the workspace's primary authoring locale. */
  primary_locale: string;
  /**
   * Ordered fallback chain the admin / operator UI walks when
   * resolving a `LocalizedText`. Non-empty list of BCP 47 tags;
   * default `["ko", "en"]`.
   */
  admin_locale_fallback: string[];
  /**
   * Ordered fallback chain the agent / Brain prompts and
   * tool-result contexts walk. Distinct from
   * `admin_locale_fallback` so a Korean-first admin surface can
   * pair with an English-first LLM context. Default `["en", "ko"]`.
   */
  llm_locale_fallback: string[];
  created_at: string;
}

export interface UpdateWorkspaceLocaleRequest {
  primary_locale: string;
  admin_locale_fallback: string[];
  llm_locale_fallback: string[];
}

export interface WorkspaceMember {
  workspace_id: string;
  user_id: string;
  role: string;
  joined_at: string;
  /** Resolved server-side via JOIN against `users` — always present. */
  email: string;
  /** Provider display name; absent when the provider didn't surface one. */
  name?: string;
  /** Avatar URL; absent when the provider didn't surface one. */
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
