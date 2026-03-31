// ---------------------------------------------------------------------------
// Admin/system types — config, users, prompts, sessions, recipes, reports
// ---------------------------------------------------------------------------

// --- System configuration (runtime-tunable from DB) ---

export interface UiConfig {
  elk_direction: string;
  elk_node_spacing: number;
  elk_layer_spacing: number;
  elk_edge_routing: string;
  worker_timeout_ms: number;
}

export interface ConfigEntry {
  key: string;
  value: string;
  data_type: string;
  description: string;
}

/** GET /api/config response: config entries grouped by category */
export type ConfigResponse = Record<string, ConfigEntry[]>;

export interface ConfigUpdateItem {
  category: string;
  key: string;
  value: string;
}

export interface ConfigUpdateRequest {
  updates: ConfigUpdateItem[];
}

// --- User Management ---

export interface UserInfo {
  id: string;
  email: string;
  name: string | null;
  picture: string | null;
  role: string;
}

// --- Prompt Templates (Admin) ---

export interface PromptTemplate {
  id: string;
  name: string;
  version: string;
  content: string;
  variables: unknown[];
  metadata: Record<string, unknown>;
  created_by: string;
  created_at: string;
  is_active: boolean;
}

// --- Agent Sessions (Audit) ---

export interface AgentSession {
  id: string;
  user_id: string;
  ontology_id: string | null;
  prompt_hash: string;
  tool_schema_hash: string;
  model_id: string;
  model_config: Record<string, unknown>;
  user_message: string;
  final_text: string | null;
  created_at: string;
  completed_at: string | null;
}

export interface AgentEvent {
  id: string;
  session_id: string;
  sequence: number;
  event_type: string;
  payload: Record<string, unknown>;
  created_at: string;
}

export type RecipeStatus = "draft" | "approved" | "deprecated";

export interface AnalysisRecipe {
  id: string;
  name: string;
  description: string;
  algorithm_type: string;
  code_template: string;
  parameters: Record<string, unknown>;
  required_columns: string[];
  output_description: string;
  created_by: string;
  created_at: string;
  version: number;
  status: RecipeStatus;
  parent_id: string | null;
}

// --- Saved Reports ---

export interface ReportParameter {
  name: string;
  type: "string" | "number" | "boolean";
  default: unknown;
  label: string;
}

export interface SavedReport {
  id: string;
  user_id: string;
  ontology_id: string;
  title: string;
  description: string | null;
  query_template: string;
  parameters: ReportParameter[];
  widget_type: string | null;
  is_public: boolean;
  created_at: string;
  updated_at: string;
}

export interface ReportCreateRequest {
  ontology_id: string;
  title: string;
  description?: string;
  query_template: string;
  parameters?: ReportParameter[];
  widget_type?: string;
  is_public?: boolean;
}

export interface ReportUpdateRequest {
  title?: string;
  description?: string;
  query_template?: string;
  parameters?: ReportParameter[];
  widget_type?: string;
  is_public?: boolean;
}

// --- Scheduled Tasks ---

export type ScheduledTaskStatus = "completed" | "error" | "running";

export interface ScheduledTask {
  id: string;
  recipe_id: string;
  ontology_id: string | null;
  cron_expression: string;
  description: string | null;
  enabled: boolean;
  last_run_at: string | null;
  next_run_at: string;
  last_status: ScheduledTaskStatus | null;
  webhook_url: string | null;
  created_by: string;
  created_at: string;
}

export interface ScheduleCreateRequest {
  cron_expression: string;
  ontology_id?: string;
  description?: string;
  webhook_url?: string;
}

export interface ScheduleUpdateRequest {
  enabled: boolean;
}
