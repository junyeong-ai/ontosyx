// ---------------------------------------------------------------------------
// Zod schemas for project API response validation
// Matches types in @/types/projects.ts exactly
// ---------------------------------------------------------------------------

import { z } from "zod";
import { OntologyIRSchema } from "./ontology";
import { OntologyQualityReportSchema } from "./quality";

export const OntologyDraftStatusSchema = z.enum([
  "analyzed",
  "designed",
  "completed",
]);

export const SourceTypeKindSchema = z.enum([
  "text",
  "csv",
  "json",
  "postgresql",
  "mysql",
  "mongodb",
  "snowflake",
  "bigquery",
  "duckdb",
  "ontology",
  "code_repository",
]);

export const SourceConfigSchema = z.object({
  source_type: SourceTypeKindSchema,
  schema_name: z.string().nullable().optional(),
  source_fingerprint: z.string().nullable().optional(),
});

export const SourceHistoryEntrySchema = z.object({
  source_type: SourceTypeKindSchema,
  added_at: z.string(),
  schema_name: z.string().nullable().optional(),
  url: z.string().optional(),
  fingerprint: z.string().optional(),
});

// SourceSchema, SourceProfile, SourceAnalysisReport are complex nested types —
// validate structure at the top level, use z.unknown() for deep internals
// that are only rendered, not programmatically consumed at the boundary.

export const ColumnDefSchema = z.object({
  name: z.string(),
  data_type: z.string(),
  nullable: z.boolean(),
});

export const ForeignKeyDefSchema = z.object({
  from_table: z.string(),
  from_column: z.string(),
  to_table: z.string(),
  to_column: z.string(),
  inferred: z.boolean().optional(),
});

export const SourceTableDefSchema = z.object({
  name: z.string(),
  columns: z.array(ColumnDefSchema),
  primary_key: z.array(z.string()),
});

export const SourceSchemaSchema = z.object({
  source_type: z.string(),
  tables: z.array(SourceTableDefSchema),
  foreign_keys: z.array(ForeignKeyDefSchema),
});

export const ColumnStatsSchema = z.object({
  column_name: z.string(),
  null_count: z.number(),
  distinct_count: z.number(),
  sample_values: z.array(z.string()),
  min_value: z.string().optional(),
  max_value: z.string().optional(),
});

export const SourceProfileSchema = z.object({
  table_profiles: z.array(z.object({
    table_name: z.string(),
    row_count: z.number(),
    column_stats: z.array(ColumnStatsSchema),
  })),
});

export const ImpliedRelationshipSchema = z.object({
  from_table: z.string(),
  from_column: z.string(),
  to_table: z.string(),
  to_column: z.string(),
  confidence: z.number(),
  pattern: z.literal("entity_id_suffix"),
  reason: z.string(),
  repo_confirmed: z.boolean(),
});

export const PiiKindSchema = z.discriminatedUnion("kind", [
  z.object({ kind: z.literal("name") }),
  z.object({ kind: z.literal("date_of_birth") }),
  z.object({ kind: z.literal("national_id"), value: z.object({ country: z.string() }) }),
  z.object({ kind: z.literal("passport") }),
  z.object({ kind: z.literal("drivers_license") }),
  z.object({ kind: z.literal("email") }),
  z.object({ kind: z.literal("phone") }),
  z.object({ kind: z.literal("address") }),
  z.object({ kind: z.literal("ip_address") }),
  z.object({ kind: z.literal("payment_card_number") }),
  z.object({ kind: z.literal("bank_account_number") }),
  z.object({ kind: z.literal("iban") }),
  z.object({ kind: z.literal("credit_card") }),
  z.object({ kind: z.literal("ssn") }),
  z.object({ kind: z.literal("medical_record_number") }),
  z.object({ kind: z.literal("insurance_id") }),
  z.object({ kind: z.literal("biometric") }),
  z.object({ kind: z.literal("geo_location") }),
  z.object({ kind: z.literal("password") }),
  z.object({ kind: z.literal("token") }),
  z.object({ kind: z.literal("custom"), value: z.string() }),
]);

export const PiiSuggestionSchema = z.object({
  table: z.string(),
  column: z.string(),
  kind: PiiKindSchema,
  confidence: z.number(),
  reason: z.string(),
});

export const RepoHintSchema = z.object({
  suggested_values: z.string(),
  source_file: z.string(),
});

export const AmbiguityColumnRefSchema = z.object({
  relation: z.string(),
  column: z.string(),
});

export const AmbiguityKindSchema = z.discriminatedUnion("kind", [
  z.object({ kind: z.literal("numeric_code") }),
  z.object({ kind: z.literal("opaque_short_code") }),
  z.object({ kind: z.literal("overloaded_name") }),
]);

export const AmbiguityContextSchema = z.object({
  id: z.string(),
  source_id: z.string(),
  column: AmbiguityColumnRefSchema,
  kind: AmbiguityKindSchema,
  sample_values: z.array(z.string()).optional(),
  distinct_estimate: z.number().nullable().optional(),
  nullable: z.boolean(),
  clarification_prompt: z.string(),
  detection_source_hash: z.string(),
  repo_hint: RepoHintSchema.optional(),
  detected_at: z.string(),
});

export const TableExclusionSuggestionSchema = z.object({
  table_name: z.string(),
  reason: z.enum(["audit_log", "temporary", "empty"]),
  row_count: z.number().optional(),
});

export const LargeSchemaWarningSchema = z.object({
  table_count: z.number(),
  recommended_max: z.number(),
});

export const RepoColumnSuggestionSchema = z.object({
  table: z.string(),
  column: z.string(),
  suggested_values: z.string(),
  source_file: z.string(),
});

export const FieldHintSchema = z.object({
  model: z.string(),
  field: z.string(),
  hint: z.string(),
  source: z.string(),
});

export const RepoFailureKindSchema = z.enum([
  "git_clone_failed",
  "local_repo_unreadable",
  "policy_rejected",
  "file_tree_failed",
  "llm_navigation_failed",
  "llm_analysis_failed",
  "timeout",
  "no_readable_files",
  "no_relevant_files",
]);

export const RepoAnalysisSummarySchema = z.object({
  status: z.enum(["complete", "partial", "skipped", "failed"]),
  failure_reason: RepoFailureKindSchema.optional(),
  framework: z.string().optional(),
  files_requested: z.number(),
  files_analyzed: z.number(),
  tree_truncated: z.boolean(),
  enums_found: z.number(),
  relationships_found: z.number(),
  columns_with_suggestions: z.number(),
  fk_confidence_upgraded: z.number(),
  commit_sha: z.string().optional(),
  field_hints: z.array(FieldHintSchema).optional(),
  domain_notes: z.array(z.string()).optional(),
});

export const WarningClassSchema = z.enum([
  "table_skipped",
  "column_sample_skipped",
  "foreign_keys_unavailable",
  "sample_values_omitted",
  "big_query_partition_filter_required",
  "big_query_clustering_filter_required",
  "big_query_jobs_create_denied",
  "postgres_permission_denied",
  "snowflake_warehouse_suspended",
  "value_set_drift_detected",
  "table_schema_drift",
  "other",
]);

export const WarningScopeSchema = z.discriminatedUnion("kind", [
  z.object({ kind: z.literal("source") }),
  z.object({ kind: z.literal("table"), name: z.string() }),
  z.object({
    kind: z.literal("column"),
    table: z.string(),
    column: z.string(),
  }),
]);

export const AnalysisWarningSchema = z.object({
  level: z.enum(["info", "warning", "error"]),
  phase: z.enum(["schema_introspection", "data_profiling"]),
  class: WarningClassSchema,
  scope: WarningScopeSchema,
  params: z.record(z.string(), z.string()).optional(),
  detail: z.string().optional(),
  group_key: z.string(),
});

export const SourceAnalysisReportSchema = z.object({
  schema_stats: z.object({
    table_count: z.number(),
    column_count: z.number(),
    declared_fk_count: z.number(),
    total_row_count: z.number(),
  }),
  implied_relationships: z.array(ImpliedRelationshipSchema),
  pii_suggestions: z.array(PiiSuggestionSchema),
  ambiguous_columns: z.array(AmbiguityContextSchema),
  table_exclusion_suggestions: z.array(TableExclusionSuggestionSchema),
  large_schema_warning: LargeSchemaWarningSchema.nullable().optional(),
  repo_suggestions: z.array(RepoColumnSuggestionSchema),
  repo_summary: RepoAnalysisSummarySchema.nullable().optional(),
  analysis_completeness: z.enum(["complete", "partial"]),
  analysis_warnings: z.array(AnalysisWarningSchema).optional(),
});

export const ConfirmedRelationshipSchema = z.object({
  from_table: z.string(),
  from_column: z.string(),
  to_table: z.string(),
  to_column: z.string(),
});

export const PiiAnnotationSchema = z.object({
  table: z.string(),
  column: z.string(),
  kind: PiiKindSchema,
});

export const ExcludedColumnSchema = z.object({
  table: z.string(),
  column: z.string(),
});

export const ColumnClarificationSchema = z.object({
  table: z.string(),
  column: z.string(),
  hint: z.string(),
});

export const DesignOptionsSchema = z.object({
  confirmed_relationships: z.array(ConfirmedRelationshipSchema).optional(),
  pii_annotations: z.array(PiiAnnotationSchema).optional(),
  excluded_columns: z.array(ExcludedColumnSchema).optional(),
  excluded_tables: z.array(z.string()).optional(),
  column_clarifications: z.array(ColumnClarificationSchema).optional(),
  partial_analysis_acknowledged: z.boolean().optional(),
});

export const GateIdSchema = z.enum([
  "column_clarifications_resolved",
  "partial_analysis_acknowledged",
  "large_schema_acknowledged",
]);

export const GateStatusSchema = z.enum(["met", "unmet"]);

export const DesignGateSchema = z.object({
  id: GateIdSchema,
  status: GateStatusSchema,
  blocks_design: z.boolean(),
  anchor: z.string().optional(),
  params: z.record(z.string(), z.string()).optional(),
});

export const AnalyzeSelectionSchema = z.discriminatedUnion("kind", [
  z.object({ kind: z.literal("all") }),
  z.object({ kind: z.literal("subset"), tables: z.array(z.string()) }),
  z.object({ kind: z.literal("staged"), tables: z.array(z.string()) }),
  z.object({ kind: z.literal("extend"), tables: z.array(z.string()) }),
  z.object({ kind: z.literal("reduce"), tables: z.array(z.string()) }),
]);

export const DeferredTableSchema = z.object({
  table: z.string(),
  reason: z.string(),
  deferred_at: z.string(),
  revisit_at: z.string().optional(),
});

export const AnalysisScopeSchema = z.object({
  included: z.array(z.string()).default([]),
  deferred: z.array(DeferredTableSchema).default([]),
  excluded_by_policy: z.array(z.string()).default([]),
  fingerprints: z.record(z.string(), z.string()).default({}),
  last_introspected_at: z.string().optional(),
});

export const OntologyDraftSchema = z.object({
  id: z.string(),
  status: OntologyDraftStatusSchema,
  revision: z.number(),
  title: z.string().nullable(),
  source_config: SourceConfigSchema,
  source_id: z.string(),
  source_data: z.string().nullable(),
  source_schema: SourceSchemaSchema.nullable(),
  source_profile: SourceProfileSchema.nullable(),
  analysis_report: SourceAnalysisReportSchema.nullable(),
  design_options: DesignOptionsSchema,
  ontology: OntologyIRSchema.nullable(),
  quality_report: OntologyQualityReportSchema.nullable(),
  parent_version_id: z.string().nullable().optional(),
  committed_version_id: z.string().nullable().optional(),
  source_history: z.array(SourceHistoryEntrySchema),
  analysis_scope: AnalysisScopeSchema.default({
    included: [],
    deferred: [],
    excluded_by_policy: [],
    fingerprints: {},
  }),
  user_id: z.string(),
  created_at: z.string(),
  updated_at: z.string(),
  analyzed_at: z.string().nullable(),
  design_gates: z.array(DesignGateSchema).default([]),
  analysis_report_status: z
    .enum(["missing", "current", "stale"])
    .default("missing"),
});

export const OntologyDraftSummarySchema = z.object({
  id: z.string(),
  status: OntologyDraftStatusSchema,
  revision: z.number(),
  title: z.string().nullable(),
  source_config: SourceConfigSchema,
  parent_version_id: z.string().nullable(),
  user_id: z.string(),
  created_at: z.string(),
  updated_at: z.string(),
  analyzed_at: z.string().nullable(),
});
