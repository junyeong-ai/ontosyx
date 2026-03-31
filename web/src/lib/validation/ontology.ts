// ---------------------------------------------------------------------------
// Zod schemas for core ontology API response validation
// Matches types in @/types/ontology.ts exactly
// ---------------------------------------------------------------------------

import { z } from "zod";

// PropertyType is recursive: { type: string; element?: PropertyType }
import type { PropertyType } from "@/types/ontology";

export const PropertyTypeSchema: z.ZodType<PropertyType> =
  z.lazy(() =>
    z.object({
      type: z.string(),
      element: PropertyTypeSchema.optional(),
    }),
  );

export const PropertyDefSchema = z.object({
  id: z.string(),
  name: z.string(),
  property_type: PropertyTypeSchema,
  nullable: z.boolean().optional(),
  default_value: z.unknown().optional(),
  description: z.string().nullish(),
  source_column: z.string().nullish(),
});

export const ConstraintDefSchema = z.union([
  z.object({ id: z.string(), type: z.literal("unique"), property_ids: z.array(z.string()) }),
  z.object({ id: z.string(), type: z.literal("exists"), property_id: z.string() }),
  z.object({ id: z.string(), type: z.literal("node_key"), property_ids: z.array(z.string()) }),
]);

export const NodeTypeDefSchema = z.object({
  id: z.string(),
  label: z.string(),
  description: z.string().nullish(),
  source_table: z.string().nullish(),
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
  description: z.string().nullish(),
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

export const OntologyIRSchema = z.object({
  id: z.string(),
  name: z.string(),
  description: z.string().nullish(),
  version: z.number(),
  node_types: z.array(NodeTypeDefSchema),
  edge_types: z.array(EdgeTypeDefSchema),
  indexes: z.array(IndexDefSchema).optional(),
});

export const SavedOntologySchema = z.object({
  id: z.string(),
  name: z.string(),
  description: z.string().nullable(),
  version: z.number(),
  ontology_ir: OntologyIRSchema,
  created_by: z.string(),
  created_at: z.string(),
});

export const CursorPageSchema = <T extends z.ZodType>(itemSchema: T) =>
  z.object({
    items: z.array(itemSchema),
    next_cursor: z.string().optional(),
  });
