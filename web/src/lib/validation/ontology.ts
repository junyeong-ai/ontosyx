// ---------------------------------------------------------------------------
// Zod schemas for core ontology API response validation
// Matches types in @/types/ontology.ts exactly
// ---------------------------------------------------------------------------

import { z } from "zod";

// Recursive ontology scalar contracts mirror the backend tagged enums.
import type { OntologyIR, PropertyType, PropertyValue } from "@/types/ontology";

export const PropertyTypeSchema: z.ZodType<PropertyType> =
  z.lazy(() =>
    z.discriminatedUnion("type", [
      z.object({ type: z.literal("bool") }).strict(),
      z.object({ type: z.literal("int") }).strict(),
      z.object({ type: z.literal("float") }).strict(),
      z.object({ type: z.literal("string") }).strict(),
      z.object({ type: z.literal("date") }).strict(),
      z.object({ type: z.literal("date_time") }).strict(),
      z.object({ type: z.literal("duration") }).strict(),
      z.object({ type: z.literal("bytes") }).strict(),
      z.object({ type: z.literal("list"), element: PropertyTypeSchema }).strict(),
      z.object({ type: z.literal("map") }).strict(),
    ]),
  );

export const PropertyValueSchema: z.ZodType<PropertyValue> =
  z.lazy(() =>
    z.discriminatedUnion("type", [
      z.object({ type: z.literal("null") }).strict(),
      z.object({ type: z.literal("bool"), value: z.boolean() }).strict(),
      z.object({ type: z.literal("int"), value: z.number() }).strict(),
      z.object({ type: z.literal("float"), value: z.number() }).strict(),
      z.object({ type: z.literal("string"), value: z.string() }).strict(),
      z.object({ type: z.literal("date"), value: z.string() }).strict(),
      z.object({ type: z.literal("date_time"), value: z.string() }).strict(),
      z.object({ type: z.literal("duration"), value: z.string() }).strict(),
      z.object({ type: z.literal("bytes"), value: z.array(z.number()) }).strict(),
      z.object({ type: z.literal("list"), value: z.array(PropertyValueSchema) }).strict(),
      z.object({ type: z.literal("map"), value: z.record(z.string(), PropertyValueSchema) }).strict(),
    ]),
  );

export const LocalizedTextSchema = z.object({
  default: z.string(),
  translations: z.record(z.string(), z.string()).optional(),
});

export const SourceLineageSchema = z.object({
  source_id: z.string().optional(),
  table: z.string(),
  primary_key: z.array(z.string()).optional(),
});

export const PropertyDefSchema = z.object({
  id: z.string(),
  name: z.string(),
  property_type: PropertyTypeSchema,
  nullable: z.boolean().optional(),
  default_value: PropertyValueSchema.optional(),
  description: LocalizedTextSchema,
  source_column: z.string().optional(),
});

export const ConstraintDefSchema = z.union([
  z.object({ id: z.string(), type: z.literal("unique"), property_ids: z.array(z.string()) }),
  z.object({ id: z.string(), type: z.literal("exists"), property_id: z.string() }),
  z.object({ id: z.string(), type: z.literal("node_key"), property_ids: z.array(z.string()) }),
]);

export const NodeTypeDefSchema = z.object({
  id: z.string(),
  label: z.string(),
  description: LocalizedTextSchema,
  source_lineage: SourceLineageSchema.optional(),
  properties: z.array(PropertyDefSchema),
  constraints: z.array(ConstraintDefSchema).optional(),
});

export const CardinalitySchema = z.enum([
  "one_to_one",
  "one_to_many",
  "many_to_one",
  "many_to_many",
]);

export const EdgeTypeDefSchema = z.object({
  id: z.string(),
  label: z.string(),
  description: LocalizedTextSchema,
  source_node_id: z.string(),
  target_node_id: z.string(),
  properties: z.array(PropertyDefSchema),
  cardinality: CardinalitySchema.optional(),
});

export const IndexDefSchema = z.object({
  id: z.string(),
  type: z.string(),
  node_id: z.string(),
  property_id: z.string().optional(),
  property_ids: z.array(z.string()).optional(),
  name: z.string().optional(),
  dimensions: z.number().optional(),
  similarity: z.string().optional(),
});

export const OntologyVersionSchema = z.object({
  number: z.number(),
  valid_from: z.string().optional(),
  valid_to: z.string().optional(),
  committed_by: z.string().optional(),
  commit_message: z.string().optional(),
});

export const OntologyIRSchema: z.ZodType<OntologyIR> = z
  .object({
    id: z.string(),
    name: z.string(),
    display_name: LocalizedTextSchema.optional(),
    description: LocalizedTextSchema,
    version: OntologyVersionSchema,
    node_types: z.array(NodeTypeDefSchema),
    edge_types: z.array(EdgeTypeDefSchema),
    indexes: z.array(IndexDefSchema).optional(),
  })
  .passthrough();

export const CurrentVersionSummarySchema = z.object({
  version_id: z.string(),
  version: z.string(),
  committed_by: z.string(),
  commit_message: z.string(),
  created_at: z.string(),
});

export const OntologyListItemSchema = z.object({
  id: z.string(),
  lineage_id: z.string(),
  name: z.string(),
  description: LocalizedTextSchema,
  created_at: z.string(),
  updated_at: z.string(),
  current_version: CurrentVersionSummarySchema.optional(),
});

export const OntologyDetailSchema = z.object({
  id: z.string(),
  lineage_id: z.string(),
  name: z.string(),
  description: LocalizedTextSchema,
  created_at: z.string(),
  updated_at: z.string(),
  current_version: CurrentVersionSummarySchema.optional(),
  ontology_ir: OntologyIRSchema.optional(),
});

export const ClientPageSchema = <T extends z.ZodType>(itemSchema: T) =>
  z.object({
    items: z.array(itemSchema),
    next_cursor: z.string().optional(),
  });
