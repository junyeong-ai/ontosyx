// ---------------------------------------------------------------------------
// Zod schemas for project API response validation
// Matches types in @/types/projects.ts exactly
// ---------------------------------------------------------------------------

import { z } from "zod";
import { OntologyIRSchema } from "./ontology";
import { OntologyQualityReportSchema } from "./quality";

export const DesignProjectStatusSchema = z.enum([
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
  "ontology",
  "code_repository",
]);

export const SourceConfigSchema = z.object({
  source_type: SourceTypeKindSchema,
  schema_name: z.string().optional(),
  source_fingerprint: z.string().optional(),
});

export const SourceHistoryEntrySchema = z.object({
  source_type: SourceTypeKindSchema,
  added_at: z.string(),
  schema_name: z.string().optional(),
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

export const SourceMappingSchema = z.object({
  node_tables: z.record(z.string(), z.string()),
  property_columns: z.record(z.string(), z.string()),
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

export const PiiFindingSchema = z.object({
  table: z.string(),
  column: z.string(),
  pii_type: z.enum(["name", "email", "phone", "birth_date", "national_id", "address", "other"]),
  detection_method: z.enum(["column_name", "value_pattern"]),
  masked_preview: z.string().optional(),
});

export const RepoSuggestionInlineSchema = z.object({
  suggested_values: z.string(),
  source_file: z.string(),
});

export const AmbiguousColumnSchema = z.object({
  table: z.string(),
  column: z.string(),
  ambiguity_type: z.enum(["numeric_code", "opaque_short_code"]),
  sample_values: z.array(z.string()),
  clarification_prompt: z.string(),
  repo_suggestion: RepoSuggestionInlineSchema.optional(),
});

export const TableExclusionSuggestionSchema = z.object({
  table_name: z.string(),
  reason: z.enum(["audit_log", "temporary", "empty"]),
  row_count: z.number().optional(),
});

export const LargeSchemaWarningSchema = z.object({
  table_count: z.number(),
  recommended_max: z.number(),
  suggestion: z.string(),
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

export const RepoAnalysisSummarySchema = z.object({
  status: z.enum(["complete", "partial", "skipped", "failed"]),
  status_reason: z.string().optional(),
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

export const AnalysisWarningSchema = z.object({
  level: z.enum(["info", "warning", "error"]),
  phase: z.enum(["schema_introspection", "data_profiling"]),
  kind: z.enum(["table_skipped", "column_skipped", "foreign_keys_unavailable", "sample_values_omitted"]),
  location: z.string(),
  message: z.string(),
});

export const SourceAnalysisReportSchema = z.object({
  schema_stats: z.object({
    table_count: z.number(),
    column_count: z.number(),
    declared_fk_count: z.number(),
    total_row_count: z.number(),
  }),
  implied_relationships: z.array(ImpliedRelationshipSchema),
  pii_findings: z.array(PiiFindingSchema),
  ambiguous_columns: z.array(AmbiguousColumnSchema),
  table_exclusion_suggestions: z.array(TableExclusionSuggestionSchema),
  large_schema_warning: LargeSchemaWarningSchema.optional(),
  repo_suggestions: z.array(RepoColumnSuggestionSchema),
  repo_summary: RepoAnalysisSummarySchema.optional(),
  analysis_completeness: z.enum(["complete", "partial"]),
  analysis_warnings: z.array(AnalysisWarningSchema),
});

export const ConfirmedRelationshipSchema = z.object({
  from_table: z.string(),
  from_column: z.string(),
  to_table: z.string(),
  to_column: z.string(),
});

export const PiiDecisionEntrySchema = z.object({
  table: z.string(),
  column: z.string(),
  decision: z.enum(["mask", "exclude", "allow"]),
});

export const ColumnClarificationSchema = z.object({
  table: z.string(),
  column: z.string(),
  hint: z.string(),
});

export const DesignOptionsSchema = z.object({
  confirmed_relationships: z.array(ConfirmedRelationshipSchema).optional(),
  pii_decisions: z.array(PiiDecisionEntrySchema).optional(),
  excluded_tables: z.array(z.string()).optional(),
  column_clarifications: z.array(ColumnClarificationSchema).optional(),
  allow_partial_source_analysis: z.boolean().optional(),
});

export const DesignProjectSchema = z.object({
  id: z.string(),
  status: DesignProjectStatusSchema,
  revision: z.number(),
  title: z.string().nullable(),
  source_config: SourceConfigSchema,
  source_data: z.string().nullable(),
  source_schema: SourceSchemaSchema.nullable(),
  source_profile: SourceProfileSchema.nullable(),
  analysis_report: SourceAnalysisReportSchema.nullable(),
  design_options: DesignOptionsSchema,
  source_mapping: SourceMappingSchema.nullable(),
  ontology: OntologyIRSchema.nullable(),
  quality_report: OntologyQualityReportSchema.nullable(),
  saved_ontology_id: z.string().nullable(),
  source_history: z.array(SourceHistoryEntrySchema),
  user_id: z.string(),
  created_at: z.string(),
  updated_at: z.string(),
  analyzed_at: z.string().nullable(),
});

export const DesignProjectSummarySchema = z.object({
  id: z.string(),
  status: DesignProjectStatusSchema,
  revision: z.number(),
  title: z.string().nullable(),
  source_config: SourceConfigSchema,
  saved_ontology_id: z.string().nullable(),
  user_id: z.string(),
  created_at: z.string(),
  updated_at: z.string(),
  analyzed_at: z.string().nullable(),
});
