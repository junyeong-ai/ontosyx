"use client";

import { useMemo } from "react";
import { useTranslations } from "next-intl";
import { Plus, RefreshCw } from "lucide-react";
import { Checkbox } from "@/components/ui/checkbox";
import { Spinner } from "@/components/ui/spinner";
import { Button } from "@/components/ui/button";
import { FormInput, FormTextarea } from "@/components/ui/form-input";
import { SourceImportPanel } from "@/components/workbench/source-import-panel";
import type { FieldErrors } from "@/hooks/use-form-with-schema";
import { cn } from "@/lib/cn";
import type { DesignSource, ProjectSource } from "@/types/api";

// ---------------------------------------------------------------------------
// Inline error helpers
// ---------------------------------------------------------------------------

interface FieldErrorProps {
  /** Translated error message; renders nothing when undefined. */
  message: string | undefined;
  /** DOM id used by `aria-describedby` to wire the field to its error. */
  id: string;
}

function FieldError({ message, id }: FieldErrorProps) {
  if (!message) return null;
  return (
    <p
      id={id}
      role="alert"
      className="mt-1 text-2xs text-danger-foreground"
    >
      {message}
    </p>
  );
}

interface FormBannerErrorProps {
  message: string | undefined;
}

function FormBannerError({ message }: FormBannerErrorProps) {
  if (!message) return null;
  return (
    <p
      role="alert"
      className="rounded border border-danger-border bg-danger-surface px-2 py-1 text-2xs text-danger-foreground"
    >
      {message}
    </p>
  );
}

// ---------------------------------------------------------------------------
// Reanalyze form
// ---------------------------------------------------------------------------

interface ReanalyzeFormProps {
  sourceType: string;
  connectionString: string;
  setConnectionString: (v: string) => void;
  schemaName: string;
  setSchemaName: (v: string) => void;
  sampleData: string;
  setSampleData: (v: string) => void;
  repoPath: string;
  setRepoPath: (v: string) => void;
  repoUrl: string;
  setRepoUrl: (v: string) => void;
  loading: boolean;
  onSubmit: () => void;
  modeledOnly: boolean;
  setModeledOnly: (v: boolean) => void;
  /** Number of tables in `analysis_scope.included` — when 0 the
   * "modeled only" checkbox is hidden (the action would 400). */
  modeledTablesAvailable: number;
  /** Field-keyed validation errors from the parent's schema. */
  errors: FieldErrors;
}

export function ReanalyzeForm({
  sourceType,
  connectionString,
  setConnectionString,
  schemaName,
  setSchemaName,
  sampleData,
  setSampleData,
  repoPath,
  setRepoPath,
  loading,
  repoUrl,
  setRepoUrl,
  onSubmit,
  modeledOnly,
  setModeledOnly,
  modeledTablesAvailable,
  errors,
}: ReanalyzeFormProps) {
  const t = useTranslations("workbench.bottomPanel.workflowForms");
  const tActions = useTranslations("workbench.bottomPanel.workflowActions");

  const localizeError = (key: string | undefined) =>
    key ? tActions(key) : undefined;

  const connError = localizeError(errors.connectionString);
  const repoError = localizeError(errors.repoUrl);
  const sampleError = localizeError(errors.sampleData);
  const formError = localizeError(errors._form);

  return (
    <div className="space-y-2 rounded-lg border border-divider bg-surface-raised p-3">
      {sourceType === "postgresql" ? (
        <div>
          <FormInput
            type="text"
            placeholder={t("postgresPlaceholder")}
            value={connectionString}
            onChange={(e) => setConnectionString(e.target.value)}
            className="font-mono"
            error={!!connError}
            aria-describedby={connError ? "reanalyze-conn-error" : undefined}
          />
          <FieldError message={connError} id="reanalyze-conn-error" />
          <FormInput
            type="text"
            placeholder={t("schemaPlaceholder")}
            value={schemaName}
            onChange={(e) => setSchemaName(e.target.value)}
            className="mt-2"
          />
        </div>
      ) : sourceType === "code_repository" ? (
        <div>
          <FormInput
            type="text"
            placeholder={t("repoUrlPlaceholder")}
            value={repoUrl}
            onChange={(e) => setRepoUrl(e.target.value)}
            className="font-mono"
            error={!!repoError}
            aria-describedby={repoError ? "reanalyze-repo-error" : undefined}
          />
          <FieldError message={repoError} id="reanalyze-repo-error" />
        </div>
      ) : (
        <div>
          <FormTextarea
            rows={4}
            placeholder={t("dataPlaceholder")}
            value={sampleData}
            onChange={(e) => setSampleData(e.target.value)}
            className="font-mono text-xs"
            error={!!sampleError}
            aria-describedby={
              sampleError ? "reanalyze-sample-error" : undefined
            }
          />
          <FieldError message={sampleError} id="reanalyze-sample-error" />
        </div>
      )}
      {sourceType !== "text" && sourceType !== "code_repository" && (
        <FormInput
          type="text"
          placeholder={t("repoPathPlaceholder")}
          value={repoPath}
          onChange={(e) => setRepoPath(e.target.value)}
        />
      )}
      {modeledTablesAvailable > 0 && sourceType !== "code_repository" && (
        <Checkbox
          checked={modeledOnly}
          onChange={(e) => setModeledOnly(e.target.checked)}
          label={
            <span className="flex-1 text-2xs">
              {t("modeledOnlyLabel", { count: modeledTablesAvailable })}
            </span>
          }
          className="rounded border border-divider bg-surface-base px-2 py-1.5 hover:bg-surface-raised"
        />
      )}
      <FormBannerError message={formError} />
      <Button
        size="sm"
        onClick={onSubmit}
        disabled={loading}
        className="w-full text-xs"
      >
        {loading ? (
          <Spinner size="xs" className="me-1.5" />
        ) : (
          <RefreshCw className="me-1.5 h-3 w-3" />
        )}
        {modeledOnly ? t("reanalyzeModeled") : t("reanalyze")}
      </Button>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Extend source form
// ---------------------------------------------------------------------------

// `as const` is load-bearing here: without it TypeScript widens
// this to `DesignSource["type"][]` and the i18n auditor / type
// guards below can't prove `snowflake` / `bigquery` can't show up
// at runtime. The SSoT for which sources the extend form actually
// renders is this tuple — keep it narrow.
export const SOURCE_TYPE_OPTIONS = [
  "text",
  "csv",
  "json",
  "duckdb",
  "code_repository",
  "postgresql",
  "mysql",
  "mongodb",
] as const satisfies readonly DesignSource["type"][];

// `ExtendSourceType` is now a precise 8-variant union. The type
// guard narrows `DesignSource["type"]` values that flow in from
// the wider wire schema onto this subset for label lookup.
type ExtendSourceType = (typeof SOURCE_TYPE_OPTIONS)[number];
function isExtendSourceType(s: DesignSource["type"]): s is ExtendSourceType {
  return (SOURCE_TYPE_OPTIONS as readonly string[]).includes(s);
}

interface ExtendFormSnapshot {
  sourceType: DesignSource["type"];
  connectionString: string;
  schemaName: string;
  database: string;
  sampleData: string;
  repoUrl: string;
  duckdbFilePath?: string;
}

/**
 * Translate the extend form's flat fields into the `ProjectSource`
 * wire shape, or `null` when the inputs aren't yet sufficient to
 * call the source-preview endpoint. Single source of truth for both
 * the panel preview (workflow-forms) and the submit path
 * (enhance-actions).
 */
export function extendSourceFromForm(
  s: ExtendFormSnapshot,
): ProjectSource | null {
  const conn = s.connectionString.trim();
  switch (s.sourceType) {
    case "postgresql":
      if (!conn) return null;
      return {
        type: "postgresql",
        connection_string: conn,
        schema: s.schemaName.trim() || "public",
      };
    case "mysql": {
      const db = s.database.trim();
      if (!conn || !db) return null;
      return { type: "mysql", connection_string: conn, schema: db };
    }
    case "mongodb": {
      const db = s.database.trim();
      if (!conn || !db) return null;
      return { type: "mongodb", connection_string: conn, database: db };
    }
    case "duckdb": {
      const fp = (s.duckdbFilePath ?? "").trim();
      if (!fp) return null;
      return { type: "duckdb", file_path: fp };
    }
    case "csv":
    case "json": {
      const data = s.sampleData.trim();
      if (!data) return null;
      return { type: s.sourceType, data };
    }
    default:
      // Text / CodeRepository / Snowflake / BigQuery — preview is
      // either trivial (Text), unsupported, or needs structured
      // input the form doesn't collect.
      return null;
  }
}

interface ExtendSourceFormProps {
  sourceType: DesignSource["type"];
  setSourceType: (v: DesignSource["type"]) => void;
  connectionString: string;
  setConnectionString: (v: string) => void;
  schemaName: string;
  setSchemaName: (v: string) => void;
  database: string;
  setDatabase: (v: string) => void;
  sampleData: string;
  setSampleData: (v: string) => void;
  repoUrl: string;
  setRepoUrl: (v: string) => void;
  duckdbFilePath?: string;
  setDuckdbFilePath?: (v: string) => void;
  importValue: import("@/components/workbench/source-import-panel").SourceImportValue;
  setImportValue: (
    v: import("@/components/workbench/source-import-panel").SourceImportValue,
  ) => void;
  loading: boolean;
  onSubmit: () => void;
  errors: FieldErrors;
}

export function ExtendSourceForm({
  sourceType,
  setSourceType,
  connectionString,
  setConnectionString,
  schemaName,
  setSchemaName,
  database,
  setDatabase,
  sampleData,
  setSampleData,
  repoUrl,
  setRepoUrl,
  duckdbFilePath,
  setDuckdbFilePath,
  importValue,
  setImportValue,
  loading,
  onSubmit,
  errors,
}: ExtendSourceFormProps) {
  const t = useTranslations("workbench.bottomPanel.workflowForms");
  const tActions = useTranslations("workbench.bottomPanel.workflowActions");
  const sourceTypes = useMemo(
    () =>
      SOURCE_TYPE_OPTIONS.map((value) => ({
        value,
        label: isExtendSourceType(value)
          ? t(`sourceTypeLabels.${value}`)
          : value,
      })),
    [t],
  );

  const localizeError = (key: string | undefined) =>
    key ? tActions(key) : undefined;

  const connError = localizeError(errors.connectionString);
  const databaseError = localizeError(errors.database);
  const fileError = localizeError(errors.duckdbFilePath);
  const repoError = localizeError(errors.repoUrl);
  const sampleError = localizeError(errors.sampleData);
  const formError = localizeError(errors._form);

  return (
    <div className="space-y-2 rounded-lg border border-info-border bg-info-surface/50 p-3">
      <h4 className="text-xs font-semibold text-info-foreground">
        {t("newSource")}
      </h4>

      {/* Source type selector */}
      <div className="flex gap-1">
        {sourceTypes.map((opt) => (
          <button type="button"
            key={opt.value}
            onClick={() => setSourceType(opt.value)}
            className={cn(
              "rounded px-2 py-0.5 text-2xs font-medium transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)]",
              sourceType === opt.value
                ? "bg-info-foreground text-foreground-onbrand"
                : "bg-surface-inset text-foreground hover:bg-surface-inset",
            )}
          >
            {opt.label}
          </button>
        ))}
      </div>
      {sourceType === "postgresql" || sourceType === "mysql" ? (
        <>
          <div>
            <FormInput
              type="text"
              placeholder={
                sourceType === "postgresql"
                  ? t("postgresPlaceholder")
                  : t("mysqlPlaceholder")
              }
              value={connectionString}
              onChange={(e) => setConnectionString(e.target.value)}
              className="font-mono"
              error={!!connError}
              aria-describedby={connError ? "extend-conn-error" : undefined}
            />
            <FieldError message={connError} id="extend-conn-error" />
          </div>
          <div>
            <FormInput
              type="text"
              placeholder={
                sourceType === "postgresql"
                  ? t("schemaPlaceholder")
                  : t("dbNamePlaceholder")
              }
              value={sourceType === "postgresql" ? schemaName : database}
              onChange={(e) =>
                sourceType === "postgresql"
                  ? setSchemaName(e.target.value)
                  : setDatabase(e.target.value)
              }
              error={sourceType === "mysql" && !!databaseError}
              aria-describedby={
                sourceType === "mysql" && databaseError
                  ? "extend-db-error"
                  : undefined
              }
            />
            {sourceType === "mysql" && (
              <FieldError message={databaseError} id="extend-db-error" />
            )}
          </div>
          <SourceImportPanel
            source={extendSourceFromForm({
              sourceType,
              connectionString,
              schemaName,
              database,
              sampleData,
              repoUrl,
              duckdbFilePath,
            })}
            value={importValue}
            onChange={setImportValue}
          />
        </>
      ) : sourceType === "mongodb" ? (
        <>
          <div>
            <FormInput
              type="text"
              placeholder={t("mongoPlaceholder")}
              value={connectionString}
              onChange={(e) => setConnectionString(e.target.value)}
              className="font-mono"
              error={!!connError}
              aria-describedby={connError ? "extend-conn-error" : undefined}
            />
            <FieldError message={connError} id="extend-conn-error" />
          </div>
          <div>
            <FormInput
              type="text"
              placeholder={t("dbNamePlaceholder")}
              value={database}
              onChange={(e) => setDatabase(e.target.value)}
              error={!!databaseError}
              aria-describedby={
                databaseError ? "extend-db-error" : undefined
              }
            />
            <FieldError message={databaseError} id="extend-db-error" />
          </div>
          <SourceImportPanel
            source={extendSourceFromForm({
              sourceType,
              connectionString,
              schemaName,
              database,
              sampleData,
              repoUrl,
              duckdbFilePath,
            })}
            value={importValue}
            onChange={setImportValue}
          />
        </>
      ) : sourceType === "duckdb" ? (
        <div>
          <FormInput
            type="text"
            placeholder={t("duckdbFilePlaceholder")}
            value={duckdbFilePath ?? ""}
            onChange={(e) => setDuckdbFilePath?.(e.target.value)}
            className="font-mono"
            error={!!fileError}
            aria-describedby={fileError ? "extend-file-error" : undefined}
          />
          <FieldError message={fileError} id="extend-file-error" />
        </div>
      ) : sourceType === "code_repository" ? (
        <div>
          <FormInput
            type="text"
            placeholder={t("repoUrlPlaceholder")}
            value={repoUrl}
            onChange={(e) => setRepoUrl(e.target.value)}
            className="font-mono"
            error={!!repoError}
            aria-describedby={repoError ? "extend-repo-error" : undefined}
          />
          <FieldError message={repoError} id="extend-repo-error" />
        </div>
      ) : (
        <div>
          <FormTextarea
            rows={4}
            placeholder={t("dataPlaceholder")}
            value={sampleData}
            onChange={(e) => setSampleData(e.target.value)}
            className="font-mono text-xs"
            error={!!sampleError}
            aria-describedby={sampleError ? "extend-sample-error" : undefined}
          />
          <FieldError message={sampleError} id="extend-sample-error" />
        </div>
      )}
      <FormBannerError message={formError} />

      <Button
        size="sm"
        onClick={onSubmit}
        disabled={loading}
        className="w-full text-xs"
      >
        {loading ? (
          <Spinner size="xs" className="me-1.5" />
        ) : (
          <Plus className="me-1.5 h-3 w-3" />
        )}
        {t("extendOntology")}
      </Button>
    </div>
  );
}
