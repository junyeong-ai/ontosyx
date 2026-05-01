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

// --- Design gates — server-evaluated checklist returned with every project

/**
 * Stable identifier for a single design-action prerequisite. New
 * variants are added on the backend (`ox_ontology::design_gate::GateId`)
 * and propagate here through the OpenAPI regeneration.
 */
export type GateId =
  | "column_clarifications_resolved"
  | "partial_analysis_acknowledged"
  | "large_schema_acknowledged";

export type GateStatus = "met" | "unmet";

/**
 * Single condition the operator must satisfy before designing.
 *
 * `params` carries interpolation values for the i18n catalogue
 * (`warnings.gate.${id}`); the backend never produces user-facing
 * prose itself. `anchor` is the DOM element id to scroll to when
 * the operator clicks the gate row.
 */
export interface DesignGate {
  id: GateId;
  status: GateStatus;
  blocks_design: boolean;
  anchor?: string | null;
  params?: Record<string, string>;
}

// --- Design Projects ---

export type DesignSource =
  | { type: "text"; data: string }
  | { type: "csv"; data: string }
  | { type: "json"; data: string }
  | { type: "postgresql"; connection_string: string; schema?: string }
  | { type: "mysql"; connection_string: string; schema: string }
  | { type: "mongodb"; connection_string: string; database: string }
  | { type: "snowflake"; account: string; user: string; password: string; warehouse: string; database: string; schema: string }
  | { type: "bigquery"; project_id: string; dataset: string; billing_project_id?: string; credentials_path?: string }
  | { type: "duckdb"; file_path: string }
  | { type: "code_repository"; url: string };

// --- Design Projects (project-based ontology lifecycle) ---

export type DesignProjectStatus = "analyzed" | "designed" | "completed";

export type SourceTypeKind = "text" | "csv" | "json" | "postgresql" | "mysql" | "mongodb" | "snowflake" | "bigquery" | "duckdb" | "ontology" | "code_repository";

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
  /**
   * Canonical `{source_type}:{fingerprint}` identity for the project's
   * primary data source. Stamped onto every ObjectMappingDef carried by
   * the ontology — federation plans and plan-cache keys both round-trip
   * through this id.
   */
  source_id: string;
  source_data: string | null;
  source_schema: SourceSchema | null;
  source_profile: SourceProfile | null;
  analysis_report: SourceAnalysisReport | null;
  design_options: DesignOptions;
  ontology: OntologyIR | null;
  quality_report: OntologyQualityReport | null;
  /** FK to `ontologies.id` — the committed identity this project was
   *  completed into. `null` until completion. */
  ontology_id: string | null;
  source_history: SourceHistoryEntry[];
  /**
   * Project-lifecycle scope — the union of tables this project has
   * modeled (`included`), the tables the operator deliberately
   * skipped (`deferred`), the policy-excluded relations the
   * platform never proposes, the per-table schema fingerprints the
   * last introspection observed, and the freshness timestamp. The
   * design-canvas header renders `included.size / total` as the
   * progress badge; the source inspector exposes the deferred list
   * so the operator can promote skipped tables one click later.
   */
  analysis_scope: AnalysisScope;
  user_id: string;
  created_at: string;
  updated_at: string;
  analyzed_at: string | null;
  /**
   * Server-evaluated design-action gates. Empty until the project
   * reaches the `analyzed` status; populated on every endpoint
   * that returns a project (`ProjectView` wrapper). The FE renders
   * the checklist directly — no client-side gate evaluation.
   */
  design_gates: DesignGate[];
  /**
   * Health of the persisted `analysis_report` blob:
   * - `missing` — no report yet (BaseOntology origin / pre-analyse).
   * - `current` — deserialises against the current wire shape;
   *   gate enforcement is fully active.
   * - `stale` — present but unparseable (older schema). Gates are
   *   skipped; the workflow surfaces a re-analyse advisory.
   */
  analysis_report_status: AnalysisReportStatus;
}

export type AnalysisReportStatus = "missing" | "current" | "stale";

export interface DesignProjectSummary {
  id: string;
  status: DesignProjectStatus;
  revision: number;
  title: string | null;
  source_config: SourceConfig;
  ontology_id: string | null;
  user_id: string;
  created_at: string;
  updated_at: string;
  analyzed_at: string | null;
}

export type ProjectSource = DesignSource;

/**
 * Wire shape for the user's analysis intent on create / extend /
 * reanalyze. Mirrors the Rust `ox_source::AnalyzeSelection` enum.
 *
 * - `all`     — full sweep of the source's tables.
 * - `subset`  — analyse only the named tables.
 * - `extend`  — grow the project's prior analysis with the named
 *   tables; existing tables are left untouched.
 */
export type AnalyzeSelection =
  | { kind: "all" }
  | { kind: "subset"; tables: string[] }
  | { kind: "extend"; tables: string[] }
  | { kind: "reduce"; tables: string[] };

/**
 * Project-lifecycle scope, mirrors the Rust
 * `ox_source::AnalysisScope`. Accumulates across every analyze /
 * extend / reanalyze pass the project runs.
 */
export interface AnalysisScope {
  included: string[];
  deferred: DeferredTable[];
  excluded_by_policy: string[];
  fingerprints: Record<string, string>;
  last_introspected_at: string | null;
}

export interface DeferredTable {
  table: string;
  reason: string;
  deferred_at: string;
  revisit_at: string | null;
}

export type RepoSource =
  | { type: "local"; path: string }
  | { type: "git_url"; url: string; branch?: string };

export type CreateProjectRequest =
  | {
      title?: string;
      origin_type: "source";
      source: ProjectSource;
      repo_source?: RepoSource;
      selection: AnalyzeSelection;
    }
  | {
      title?: string;
      origin_type: "base_ontology";
      base_ontology_id: string;
    };

export interface UpdateProjectDecisionsRequest {
  design_options: DesignOptions;
  revision: number;
}

export interface DesignProjectRequest {
  revision: number;
  context?: string;
  acknowledge_large_schema?: boolean;
}

export interface ReanalyzeProjectRequest {
  source: ProjectSource;
  revision: number;
  repo_source?: RepoSource;
  selection: AnalyzeSelection;
}

export interface RefineProjectRequest {
  revision: number;
  additional_context?: string;
}

export interface EditProjectRequest {
  revision: number;
  user_request: string;
  dry_run?: boolean;
}

export interface EditProjectResponse {
  project: DesignProject | null;
  commands: OntologyCommand[];
  explanation: string;
}

export interface ExtendProjectRequest {
  revision: number;
  source: DesignSource;
  selection: AnalyzeSelection;
}

export interface ExtendProjectResponse {
  project: DesignProject;
  reconcile_report: ReconcileReport;
}

// --- Source preview (cheap table listing) ---

export interface PreviewSourceRequest {
  source: ProjectSource;
}

export interface PreviewTableSummary {
  name: string;
  estimated_row_count: number | null;
  column_count: number;
  last_modified: string | null;
}

export interface PreviewSourceResponse {
  source_type: string;
  tables: PreviewTableSummary[];
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

export type PiiKind =
  | { kind: "name" }
  | { kind: "date_of_birth" }
  | { kind: "national_id"; value: { country: string } }
  | { kind: "passport" }
  | { kind: "drivers_license" }
  | { kind: "email" }
  | { kind: "phone" }
  | { kind: "address" }
  | { kind: "ip_address" }
  | { kind: "payment_card_number" }
  | { kind: "bank_account_number" }
  | { kind: "iban" }
  | { kind: "credit_card" }
  | { kind: "ssn" }
  | { kind: "medical_record_number" }
  | { kind: "insurance_id" }
  | { kind: "biometric" }
  | { kind: "geo_location" }
  | { kind: "password" }
  | { kind: "token" }
  | { kind: "custom"; value: string };

export interface PiiAnnotation {
  table: string;
  column: string;
  kind: PiiKind;
}

export interface ExcludedColumn {
  table: string;
  column: string;
}

export interface ColumnClarification {
  table: string;
  column: string;
  hint: string;
}

export interface DesignOptions {
  confirmed_relationships?: ConfirmedRelationship[];
  pii_annotations?: PiiAnnotation[];
  excluded_columns?: ExcludedColumn[];
  excluded_tables?: string[];
  column_clarifications?: ColumnClarification[];
  partial_analysis_acknowledged?: boolean;
  large_schema_acknowledged?: boolean;
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

/**
 * Stable warning classification produced by the analyzer / each
 * adapter. New variants are added on the backend
 * (`ox_ontology::source_analysis::WarningClass`); the FE i18n
 * catalogue keys warning copy by `class`. Keep in sync with the
 * generated type in `api.generated.ts`.
 */
export type WarningClass =
  | "table_skipped"
  | "column_sample_skipped"
  | "foreign_keys_unavailable"
  | "sample_values_omitted"
  | "bigquery_partition_filter_required"
  | "bigquery_clustering_filter_required"
  | "bigquery_jobs_create_denied"
  | "postgres_permission_denied"
  | "snowflake_warehouse_suspended"
  | "other";

export type WarningLevel = "info" | "warning" | "error";

/** Where in the source the warning originated. */
export type WarningScope =
  | { kind: "source" }
  | { kind: "table"; name: string }
  | { kind: "column"; table: string; column: string };

export interface AnalysisWarning {
  level: WarningLevel;
  phase: AnalysisPhase;
  class: WarningClass;
  scope: WarningScope;
  /** Interpolation arguments for the FE i18n catalogue. */
  params?: Record<string, string>;
  /** Raw provider error text — operator drilldown only. */
  detail?: string | null;
  /**
   * Deterministic fingerprint (`class:scope.table` or
   * `class:source`) the FE uses to coalesce N warnings of the same
   * class affecting the same table into a single grouped card.
   */
  group_key: string;
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

export interface PiiSuggestion {
  table: string;
  column: string;
  kind: PiiKind;
  confidence: number;
  reason: string;
}

/**
 * Persistent ambiguity context — matches the Rust
 * `ox_ontology::ambiguity::AmbiguityContext`. One row per
 * `(source_id, relation, column)`; a resolution attached at
 * `context_id` carries the chosen interpretation.
 */
export type AmbiguityKind =
  | { kind: "numeric_code" }
  | { kind: "opaque_short_code" }
  | { kind: "overloaded_name" };

export interface RepoHint {
  suggested_values: string;
  source_file: string;
}

export interface AmbiguityColumnRef {
  relation: string;
  column: string;
}

export interface AmbiguityContext {
  id: string;
  source_id: string;
  column: AmbiguityColumnRef;
  kind: AmbiguityKind;
  sample_values: string[];
  distinct_estimate?: number;
  nullable: boolean;
  clarification_prompt: string;
  detection_source_hash: string;
  repo_hint?: RepoHint;
  detected_at: string;
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
}

export type RepoAnalysisStatus = "complete" | "partial" | "skipped" | "failed";

/**
 * Structured cause of a repo-enrichment failure or skip. Mirrors the
 * Rust `RepoFailureKind` enum; the FE renders a localised hint per
 * variant via the `repoFailure.<kind>` i18n key.
 */
export type RepoFailureKind =
  | "git_clone_failed"
  | "local_repo_unreadable"
  | "policy_rejected"
  | "file_tree_failed"
  | "llm_navigation_failed"
  | "llm_analysis_failed"
  | "timeout"
  | "no_readable_files"
  | "no_relevant_files";

export interface FieldHint {
  model: string;
  field: string;
  hint: string;
  source: string;
}

export interface RepoAnalysisSummary {
  status: RepoAnalysisStatus;
  failure_reason?: RepoFailureKind;
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
  pii_suggestions: PiiSuggestion[];
  ambiguous_columns: AmbiguityContext[];
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

export interface ColumnStats {
  column_name: string;
  null_count: number;
  distinct_count: number;
  sample_values: string[];
  min_value?: string;
  max_value?: string;
  /**
   * Set when sample collection flagged this column as likely PII by
   * name heuristic and dropped raw values. The FE renders a
   * "Redacted: <kind>" badge in place of the distribution detail.
   */
  pii_redacted?: PiiSuspectKind | null;
}

export type PiiSuspectKind =
  | { kind: "email" }
  | { kind: "phone" }
  | { kind: "name" }
  | { kind: "address" }
  | { kind: "national_id" }
  | { kind: "payment_card" }
  | { kind: "password" }
  | { kind: "token" }
  | { kind: "other"; value: string };

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
  ontology_lineage_id: string;
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
