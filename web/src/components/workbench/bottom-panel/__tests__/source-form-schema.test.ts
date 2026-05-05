import { describe, it, expect } from "vitest";

import {
  ExtendSourceFormSchema,
  ReanalyzeSourceFormSchema,
  toDesignSource,
} from "../source-form-schema";

describe("ExtendSourceFormSchema", () => {
  it("accepts postgresql with connection string and defaults schema to 'public' when blank", () => {
    const parsed = ExtendSourceFormSchema.parse({
      sourceType: "postgresql",
      connectionString: "postgres://host/db",
      schemaName: "",
    });
    if (parsed.sourceType !== "postgresql") throw new Error("expected pg");
    expect(parsed.connectionString).toBe("postgres://host/db");
    expect(toDesignSource(parsed)).toEqual({
      type: "postgresql",
      connection_string: "postgres://host/db",
      schema: "public",
    });
  });

  it("rejects postgresql with empty connection string and surfaces the i18n key", () => {
    const result = ExtendSourceFormSchema.safeParse({
      sourceType: "postgresql",
      connectionString: "   ",
      schemaName: "",
    });
    expect(result.success).toBe(false);
    if (result.success) return;
    const issue = result.error.issues.find((i) =>
      i.path.includes("connectionString"),
    );
    expect(issue?.message).toBe("errors.connectionStringRequired");
  });

  it("requires both connection string and database for mysql", () => {
    const noDb = ExtendSourceFormSchema.safeParse({
      sourceType: "mysql",
      connectionString: "mysql://host",
      database: "",
    });
    expect(noDb.success).toBe(false);
    if (!noDb.success) {
      expect(
        noDb.error.issues.find((i) => i.path.includes("database"))?.message,
      ).toBe("errors.databaseRequired");
    }

    const ok = ExtendSourceFormSchema.parse({
      sourceType: "mysql",
      connectionString: "mysql://host",
      database: "app",
    });
    expect(toDesignSource(ok)).toEqual({
      type: "mysql",
      connection_string: "mysql://host",
      schema: "app",
    });
  });

  it("requires file path for duckdb", () => {
    const empty = ExtendSourceFormSchema.safeParse({
      sourceType: "duckdb",
      duckdbFilePath: "",
    });
    expect(empty.success).toBe(false);
    if (!empty.success) {
      expect(empty.error.issues[0].message).toBe("errors.filePathRequired");
    }
    const ok = ExtendSourceFormSchema.parse({
      sourceType: "duckdb",
      duckdbFilePath: "/tmp/sample.duckdb",
    });
    expect(toDesignSource(ok)).toEqual({
      type: "duckdb",
      file_path: "/tmp/sample.duckdb",
    });
  });

  it("requires URL for code_repository", () => {
    const empty = ExtendSourceFormSchema.safeParse({
      sourceType: "code_repository",
      repoUrl: "",
    });
    expect(empty.success).toBe(false);
    if (!empty.success) {
      expect(empty.error.issues[0].message).toBe("errors.repoUrlRequired");
    }
  });

  it("requires sample data for text/csv/json variants", () => {
    for (const sourceType of ["text", "csv", "json"] as const) {
      const empty = ExtendSourceFormSchema.safeParse({
        sourceType,
        sampleData: "",
      });
      expect(empty.success).toBe(false);
      if (!empty.success) {
        expect(empty.error.issues[0].message).toBe(
          "errors.sourceDataRequired",
        );
      }
      const ok = ExtendSourceFormSchema.parse({
        sourceType,
        sampleData: "sample",
      });
      expect(toDesignSource(ok)).toEqual({
        type: sourceType,
        data: "sample",
      });
    }
  });

  it("rejects snowflake with the unsupported i18n key", () => {
    const result = ExtendSourceFormSchema.safeParse({
      sourceType: "snowflake",
    });
    expect(result.success).toBe(false);
    if (!result.success) {
      expect(result.error.issues[0].message).toBe(
        "errors.snowflakeExtendUnsupported",
      );
    }
  });

  it("rejects bigquery with the unsupported i18n key", () => {
    const result = ExtendSourceFormSchema.safeParse({
      sourceType: "bigquery",
    });
    expect(result.success).toBe(false);
    if (!result.success) {
      expect(result.error.issues[0].message).toBe(
        "errors.bigqueryExtendUnsupported",
      );
    }
  });
});

describe("ReanalyzeSourceFormSchema", () => {
  it("accepts the project's existing source families", () => {
    const pg = ReanalyzeSourceFormSchema.parse({
      sourceType: "postgresql",
      connectionString: "postgres://h/d",
      schemaName: "public",
    });
    expect(toDesignSource(pg).type).toBe("postgresql");

    const repo = ReanalyzeSourceFormSchema.parse({
      sourceType: "code_repository",
      repoUrl: "git@host:org/repo.git",
    });
    expect(toDesignSource(repo)).toEqual({
      type: "code_repository",
      url: "git@host:org/repo.git",
    });

    const text = ReanalyzeSourceFormSchema.parse({
      sourceType: "text",
      sampleData: "hello",
    });
    expect(toDesignSource(text)).toEqual({ type: "text", data: "hello" });
  });

  it("rejects mysql/mongodb/duckdb at the discriminator level (Reanalyze does not support them)", () => {
    const mysql = ReanalyzeSourceFormSchema.safeParse({
      sourceType: "mysql",
      connectionString: "x",
      database: "y",
    });
    expect(mysql.success).toBe(false);
    const duck = ReanalyzeSourceFormSchema.safeParse({
      sourceType: "duckdb",
      duckdbFilePath: "/x",
    });
    expect(duck.success).toBe(false);
  });
});
