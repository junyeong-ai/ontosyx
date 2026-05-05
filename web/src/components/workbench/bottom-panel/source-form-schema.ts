// Source-form validation — schemas + mapper for the Extend / Reanalyze
// flows.
//
// The two surfaces share a discriminator (`sourceType`) but accept
// different subsets of source kinds: Reanalyze stays with the
// project's existing source family (postgresql / code_repository /
// sample text-csv-json), while Extend additionally allows mysql /
// mongodb / duckdb. Snowflake and BigQuery have no Extend
// implementation and are rejected at the schema's refinement layer
// — the failure surfaces through the same field-error channel
// instead of an out-of-band capability check inside the handler.
//
// Adding a new source family is a single schema variant + a `case`
// in `toDesignSource()`; no caller of `EnhanceActions` needs to be
// touched. The shape mirrors the FE form-state names so `submit()`
// can be called with the workflow form-state object directly.

import { z } from "zod";

import type { DesignSource } from "@/types/api";

// FE form-state mirror — what the workflow form-state object holds
// while the user is filling in source-connection details. Always
// has every field as `string`, even the ones irrelevant to the
// currently-selected source type. The `buildExtendInput` /
// `buildReanalyzeInput` helpers below project this flat record onto
// the discriminated-union shape the schema expects.
export interface SourceFormSnapshot {
  connectionString: string;
  schemaName: string;
  database: string;
  duckdbFilePath: string;
  repoUrl: string;
  sampleData: string;
}

const requiredTrim = (key: string) =>
  z.string().trim().min(1, { message: key });
const optionalTrim = z.string().trim();

const PostgresVariant = z.object({
  sourceType: z.literal("postgresql"),
  connectionString: requiredTrim("errors.connectionStringRequired"),
  schemaName: optionalTrim,
});

const MySQLVariant = z.object({
  sourceType: z.literal("mysql"),
  connectionString: requiredTrim("errors.connectionStringRequired"),
  database: requiredTrim("errors.databaseRequired"),
});

const MongoVariant = z.object({
  sourceType: z.literal("mongodb"),
  connectionString: requiredTrim("errors.connectionStringRequired"),
  database: requiredTrim("errors.databaseRequired"),
});

const DuckDBVariant = z.object({
  sourceType: z.literal("duckdb"),
  duckdbFilePath: requiredTrim("errors.filePathRequired"),
});

const RepoVariant = z.object({
  sourceType: z.literal("code_repository"),
  repoUrl: requiredTrim("errors.repoUrlRequired"),
});

const SampleTextVariant = z.object({
  sourceType: z.literal("text"),
  sampleData: requiredTrim("errors.sourceDataRequired"),
});
const SampleCsvVariant = z.object({
  sourceType: z.literal("csv"),
  sampleData: requiredTrim("errors.sourceDataRequired"),
});
const SampleJsonVariant = z.object({
  sourceType: z.literal("json"),
  sampleData: requiredTrim("errors.sourceDataRequired"),
});

const SnowflakeRejectVariant = z
  .object({ sourceType: z.literal("snowflake") })
  .refine(() => false, { message: "errors.snowflakeExtendUnsupported" });

const BigQueryRejectVariant = z
  .object({ sourceType: z.literal("bigquery") })
  .refine(() => false, { message: "errors.bigqueryExtendUnsupported" });

export const ExtendSourceFormSchema = z.discriminatedUnion("sourceType", [
  PostgresVariant,
  MySQLVariant,
  MongoVariant,
  DuckDBVariant,
  RepoVariant,
  SampleTextVariant,
  SampleCsvVariant,
  SampleJsonVariant,
  SnowflakeRejectVariant,
  BigQueryRejectVariant,
]);

export const ReanalyzeSourceFormSchema = z.discriminatedUnion("sourceType", [
  PostgresVariant,
  RepoVariant,
  SampleTextVariant,
  SampleCsvVariant,
  SampleJsonVariant,
]);

export type ExtendSourceFormInput = z.input<typeof ExtendSourceFormSchema>;
export type ReanalyzeSourceFormInput = z.input<typeof ReanalyzeSourceFormSchema>;

// The schema output preserves every discriminator branch (zod 3
// keeps refine-to-false variants in the inferred type), so the
// mapper accepts the wider union and rejects the unsupported
// branches with an explicit unreachable. In practice they never
// reach the mapper — the schema's refinement rejects them before
// `onValid` is invoked.
export type ValidatedSourceFormValue =
  | z.infer<typeof ExtendSourceFormSchema>
  | z.infer<typeof ReanalyzeSourceFormSchema>;

/**
 * Project the flat form-state snapshot onto the shape the Extend
 * schema expects. Keeps `enhance-actions` free of `as` casts —
 * each branch returns a typed discriminated-union variant that the
 * schema's `discriminatedUnion` accepts directly.
 */
export function buildExtendInput(
  sourceType: DesignSource["type"] | "snowflake" | "bigquery",
  snapshot: SourceFormSnapshot,
): ExtendSourceFormInput {
  switch (sourceType) {
    case "postgresql":
      return {
        sourceType: "postgresql",
        connectionString: snapshot.connectionString,
        schemaName: snapshot.schemaName,
      };
    case "mysql":
      return {
        sourceType: "mysql",
        connectionString: snapshot.connectionString,
        database: snapshot.database,
      };
    case "mongodb":
      return {
        sourceType: "mongodb",
        connectionString: snapshot.connectionString,
        database: snapshot.database,
      };
    case "duckdb":
      return {
        sourceType: "duckdb",
        duckdbFilePath: snapshot.duckdbFilePath,
      };
    case "code_repository":
      return { sourceType: "code_repository", repoUrl: snapshot.repoUrl };
    case "text":
    case "csv":
    case "json":
      return { sourceType, sampleData: snapshot.sampleData };
    case "snowflake":
      return { sourceType: "snowflake" };
    case "bigquery":
      return { sourceType: "bigquery" };
  }
}

/**
 * Reanalyze accepts a tighter set than Extend — postgresql,
 * code_repository, text/csv/json. Other source-type values are
 * coerced to `text` (the default fallback in the form state) so
 * the schema's discriminator stays exhaustive without an open
 * `string` widening.
 */
export function buildReanalyzeInput(
  sourceType: DesignSource["type"] | "ontology",
  snapshot: SourceFormSnapshot,
): ReanalyzeSourceFormInput {
  switch (sourceType) {
    case "postgresql":
      return {
        sourceType: "postgresql",
        connectionString: snapshot.connectionString,
        schemaName: snapshot.schemaName,
      };
    case "code_repository":
      return { sourceType: "code_repository", repoUrl: snapshot.repoUrl };
    case "csv":
    case "json":
    case "text":
      return { sourceType, sampleData: snapshot.sampleData };
    default:
      // mysql / mongodb / duckdb / snowflake / bigquery / ontology —
      // none are project-source families that Reanalyze accepts.
      // Fall through to text so the schema can produce a
      // discriminated rejection if the form somehow reaches this
      // path.
      return { sourceType: "text", sampleData: snapshot.sampleData };
  }
}

export function toDesignSource(v: ValidatedSourceFormValue): DesignSource {
  switch (v.sourceType) {
    case "postgresql":
      return {
        type: "postgresql",
        connection_string: v.connectionString,
        schema: v.schemaName || "public",
      };
    case "mysql":
      return {
        type: "mysql",
        connection_string: v.connectionString,
        schema: v.database,
      };
    case "mongodb":
      return {
        type: "mongodb",
        connection_string: v.connectionString,
        database: v.database,
      };
    case "duckdb":
      return { type: "duckdb", file_path: v.duckdbFilePath };
    case "code_repository":
      return { type: "code_repository", url: v.repoUrl };
    case "text":
    case "csv":
    case "json":
      return { type: v.sourceType, data: v.sampleData };
    case "snowflake":
    case "bigquery":
      throw new Error(
        `toDesignSource: ${v.sourceType} is rejected by the schema refinement and should never reach this branch`,
      );
  }
}
