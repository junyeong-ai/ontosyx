// ---------------------------------------------------------------------------
// Design project types — project lifecycle, analysis, source introspection
// ---------------------------------------------------------------------------

import type {
  OntologyIR,
  OntologyCommand,
} from "./ontology";

import type {
  OntologyQualityReport,
  ReconcileReport,
} from "./quality";

// --- Design Projects ---

export type DesignSource =
  | { type: "text"; data: string }
  | { type: "csv"; data: string }
  | { type: "json"; data: string }
  | { type: "postgresql"; connection_string: string; schema?: string }
  | { type: "mysql"; connection_string: string; schema: string }
  | { type: "mongodb"; connection_string: string; database: string }
  | { type: "code_repository"; url: string };

// --- Design Projects (project-based ontology lifecycle) ---

export type DesignProjectStatus = "analyzed" | "designed" | "completed";

export type SourceTypeKind = "text" | "csv" | "json" | "postgresql" | "mysql" | "mongodb" | "ontology" | "code_repository";

export interface SourceConfig {
  source_type: SourceTypeKind;
  schema_name?: string;
  source_fingerprint?: string;
}

export interface SourceHistoryEntry {
  source_type: SourceTypeKind;
  added_at: string;
  schema_name?: string;
  url?: string;
  fingerprint?: string;
}

export interface DesignProject {
  id: string;
  status: DesignProjectStatus;
  revision: number;
  title: string | null;
  source_config: SourceConfig;
  source_data: string | null;
  source_schema: SourceSchema | null;
  source_profile: SourceProfile | null;
  analysis_report: SourceAnalysisReport | null;
  design_options: DesignOptions;
  source_mapping: SourceMapping | null;
  ontology: OntologyIR | null;
  quality_report: OntologyQualityReport | null;
  saved_ontology_id: string | null;
  source_history: SourceHistoryEntry[];
  user_id: string;
  created_at: string;
  updated_at: string;
  analyzed_at: string | null;
}

export interface DesignProjectSummary {
  id: string;
  status: DesignProjectStatus;
  revision: number;
  title: string | null;
  source_config: SourceConfig;
  saved_ontology_id: string | null;
  user_id: string;
  created_at: string;
  updated_at: string;
  analyzed_at: string | null;
}

export type ProjectSource = DesignSource;

export type RepoSource =
  | { type: "local"; path: string }
  | { type: "git_url"; url: string; branch?: string };

export type CreateProjectRequest =
  | {
      title?: string;
      origin_type: "source";
      source: ProjectSource;
      repo_source?: RepoSource;
    }
  | {
      title?: string;
      origin_type: "base_ontology";
      base_ontology_id: string;
    };

export interface UpdateDecisionsRequest {
  design_options: DesignOptions;
  revision: number;
}

export interface DesignProjectRequest {
  revision: number;
  context?: string;
  acknowledge_large_schema?: boolean;
}

export interface ProjectReanalyzeRequest {
  source: ProjectSource;
  revision: number;
  repo_source?: RepoSource;
}

export interface RefineProjectRequest {
  revision: number;
  additional_context?: string;
}

export interface ProjectEditRequest {
  revision: number;
  user_request: string;
  dry_run?: boolean;
}

export interface ProjectEditResponse {
  project: DesignProject | null;
  commands: OntologyCommand[];
  explanation: string;
}

export interface ProjectExtendRequest {
  revision: number;
  source: DesignSource;
}

export interface ProjectExtendResponse {
  project: DesignProject;
  reconcile_report: ReconcileReport;
}

export interface CompleteProjectRequest {
  revision: number;
  name: string;
  description?: string;
  acknowledge_quality_risks?: boolean;
}

export interface ConfirmedRelationship {
  from_table: string;
  from_column: string;
  to_table: string;
  to_column: string;
}

export type PiiDecision = "mask" | "exclude" | "allow";

export interface PiiDecisionEntry {
  table: string;
  column: string;
  decision: PiiDecision;
}

export interface ColumnClarification {
  table: string;
  column: string;
  hint: string;
}

export interface DesignOptions {
  confirmed_relationships?: ConfirmedRelationship[];
  pii_decisions?: PiiDecisionEntry[];
  excluded_tables?: string[];
  column_clarifications?: ColumnClarification[];
  allow_partial_source_analysis?: boolean;
}

export interface RepoColumnSuggestion {
  table: string;
  column: string;
  suggested_values: string;
  source_file: string;
}

export interface SchemaStats {
  table_count: number;
  column_count: number;
  declared_fk_count: number;
  total_row_count: number;
}

export type AnalysisCompleteness = "complete" | "partial";

export type AnalysisPhase = "schema_introspection" | "data_profiling";

export type AnalysisWarningKind =
  | "table_skipped"
  | "column_skipped"
  | "foreign_keys_unavailable"
  | "sample_values_omitted";

export type WarningLevel = "info" | "warning" | "error";

export interface AnalysisWarning {
  level: WarningLevel;
  phase: AnalysisPhase;
  kind: AnalysisWarningKind;
  location: string;
  message: string;
}

export type ImpliedFkPattern = "entity_id_suffix";

export interface ImpliedRelationship {
  from_table: string;
  from_column: string;
  to_table: string;
  to_column: string;
  confidence: number;
  pattern: ImpliedFkPattern;
  reason: string;
  repo_confirmed: boolean;
}

export type PiiType =
  | "name"
  | "email"
  | "phone"
  | "birth_date"
  | "national_id"
  | "address"
  | "other";

export type PiiDetectionMethod = "column_name" | "value_pattern";

export interface PiiFinding {
  table: string;
  column: string;
  pii_type: PiiType;
  detection_method: PiiDetectionMethod;
  masked_preview?: string;
}

export type AmbiguityType = "numeric_code" | "opaque_short_code";

export interface RepoSuggestion {
  suggested_values: string;
  source_file: string;
}

export interface AmbiguousColumn {
  table: string;
  column: string;
  ambiguity_type: AmbiguityType;
  sample_values: string[];
  clarification_prompt: string;
  repo_suggestion?: RepoSuggestion;
}

export type TableExclusionReason = "audit_log" | "temporary" | "empty";

export interface TableExclusionSuggestion {
  table_name: string;
  reason: TableExclusionReason;
  row_count?: number;
}

export interface LargeSchemaWarning {
  table_count: number;
  recommended_max: number;
  suggestion: string;
}

export type RepoAnalysisStatus = "complete" | "partial" | "skipped" | "failed";

export interface FieldHint {
  model: string;
  field: string;
  hint: string;
  source: string;
}

export interface RepoAnalysisSummary {
  status: RepoAnalysisStatus;
  status_reason?: string;
  framework?: string;
  files_requested: number;
  files_analyzed: number;
  tree_truncated: boolean;
  enums_found: number;
  relationships_found: number;
  columns_with_suggestions: number;
  fk_confidence_upgraded: number;
  commit_sha?: string;
  field_hints?: FieldHint[];
  domain_notes?: string[];
}

export interface SourceAnalysisReport {
  schema_stats: SchemaStats;
  implied_relationships: ImpliedRelationship[];
  pii_findings: PiiFinding[];
  ambiguous_columns: AmbiguousColumn[];
  table_exclusion_suggestions: TableExclusionSuggestion[];
  large_schema_warning?: LargeSchemaWarning;
  repo_suggestions: RepoColumnSuggestion[];
  repo_summary?: RepoAnalysisSummary;
  analysis_completeness: AnalysisCompleteness;
  analysis_warnings: AnalysisWarning[];
}

// --- Source introspection (returned only for DB sources) ---

export interface ColumnDef {
  name: string;
  data_type: string;
  nullable: boolean;
}

export interface ForeignKeyDef {
  from_table: string;
  from_column: string;
  to_table: string;
  to_column: string;
  /** True if inferred from document structure (e.g., JSON nesting) rather than declared in source */
  inferred?: boolean;
}

export interface SourceTableDef {
  name: string;
  columns: ColumnDef[];
  primary_key: string[];
}

export interface SourceSchema {
  source_type: string;
  tables: SourceTableDef[];
  foreign_keys: ForeignKeyDef[];
}

export interface SourceMapping {
  node_tables: Record<string, string>;
  property_columns: Record<string, string>;
}

export interface ColumnStats {
  column_name: string;
  null_count: number;
  distinct_count: number;
  sample_values: string[];
  min_value?: string;
  max_value?: string;
}

export interface TableProfile {
  table_name: string;
  row_count: number;
  column_stats: ColumnStats[];
}

export interface SourceProfile {
  table_profiles: TableProfile[];
}

// --- Schema Deploy ---

export interface ProjectDeployRequest {
  dry_run?: boolean;
}

export interface ProjectDeployResponse {
  statements: string[];
  executed: boolean;
}

// --- Schema Migration ---

export interface ProjectMigrateRequest {
  dry_run?: boolean;
}

export interface ProjectMigrateResponse {
  up: string[];
  down: string[];
  warnings: string[];
  breaking_changes: string[];
  executed: boolean;
}

// --- Load Plan ---

export interface ProjectLoadPlanResponse {
  plan: LoadPlan;
}

export interface LoadPlan {
  id: string;
  ontology_id: string;
  ontology_version: number;
  source: unknown;
  steps: LoadStep[];
  batch_config: {
    batch_size: number;
    parallelism: number;
    transactional: boolean;
  };
}

export interface LoadStep {
  order: number;
  depends_on: number[];
  operation: unknown;
  description: string;
}

export interface ProjectLoadCompileRequest {
  plan: LoadPlan;
}

export interface ProjectLoadCompileResponse {
  statements: string[];
}
