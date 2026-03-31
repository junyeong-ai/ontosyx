// ---------------------------------------------------------------------------
// Zod schemas for quality report API response validation
// Matches types in @/types/quality.ts exactly
// ---------------------------------------------------------------------------

import { z } from "zod";

export const QualityGapSeveritySchema = z.enum(["high", "medium", "low"]);

export const QualityGapCategorySchema = z.enum([
  "opaque_enum_value",
  "numeric_enum_code",
  "single_value_bias",
  "small_sample",
  "missing_description",
  "sparse_property",
  "unmapped_source_table",
  "missing_foreign_key_edge",
  "missing_containment_edge",
  "unmapped_source_column",
  "duplicate_edge",
  "orphan_node",
  "property_type_inconsistency",
  "hub_node",
  "overloaded_property",
  "self_referential_edge",
]);

export const QualityGapRefSchema = z.union([
  z.object({ ref_type: z.literal("node"), node_id: z.string(), label: z.string() }),
  z.object({ ref_type: z.literal("node_property"), node_id: z.string(), property_id: z.string(), label: z.string(), property_name: z.string() }),
  z.object({ ref_type: z.literal("edge"), edge_id: z.string(), label: z.string() }),
  z.object({ ref_type: z.literal("edge_property"), edge_id: z.string(), property_id: z.string(), label: z.string(), property_name: z.string() }),
  z.object({ ref_type: z.literal("source_table"), table: z.string() }),
  z.object({ ref_type: z.literal("source_column"), table: z.string(), column: z.string() }),
  z.object({ ref_type: z.literal("source_foreign_key"), from_table: z.string(), from_column: z.string(), to_table: z.string(), to_column: z.string() }),
]);

export const QualityGapSchema = z.object({
  severity: QualityGapSeveritySchema,
  category: QualityGapCategorySchema,
  location: QualityGapRefSchema,
  issue: z.string(),
  suggestion: z.string(),
});

export const QualityConfidenceSchema = z.enum(["high", "medium", "low"]);

export const OntologyQualityReportSchema = z.object({
  confidence: QualityConfidenceSchema,
  gaps: z.array(QualityGapSchema),
});
