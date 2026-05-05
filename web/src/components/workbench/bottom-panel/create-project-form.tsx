"use client";

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslations } from "next-intl";
import { HugeiconsIcon } from "@hugeicons/react";
import {
  PlusSignIcon,
  CancelCircleIcon,
  ArrowLeft01Icon,
} from "@hugeicons/core-free-icons";
import { toast } from "@/components/ui/toast";

import { createProject } from "@/lib/api";
import { isGitUrl } from "@/lib/git-url";
import { Button } from "@/components/ui/button";
import { FormInput, FormSelect, FormTextarea, SecretInput } from "@/components/ui/form-input";
import { FormField } from "@/components/ui/form-field";
import { Spinner } from "@/components/ui/spinner";
import { TableSelector } from "@/components/source/table-selector";
import { useSourcePreview } from "@/hooks/use-source-preview";
import type {
  AnalyzeSelection,
  DesignProject,
  DesignSource,
} from "@/types/api";
import type { GenerateSourceType } from "./design-panel-shared";

/**
 * Two-phase project creation:
 *
 * 1. **connection** — user enters source-connection details. For DB
 *    sources (PG / MySQL / Mongo / Snowflake / BQ) the primary action
 *    is "Preview tables" which lists the catalogue without paying any
 *    profiling or sampling cost. For file sources (CSV / JSON / DuckDB)
 *    and code repositories there is no per-table choice to make, so
 *    the primary action stays "Create project".
 *
 * 2. **select_tables** — DB sources only. The user curates which
 *    tables to analyse before any INFORMATION_SCHEMA / sampling
 *    query fires. The selection is passed as
 *    `AnalyzeSelection::Subset` to the server, so cost scales with
 *    the user's deliberate pick rather than the dataset's full
 *    surface.
 */

type Phase = "connection" | "select_tables";

const DB_SOURCE_TYPES = new Set<GenerateSourceType>([
  "postgresql",
  "mysql",
  "mongodb",
  "snowflake",
  "bigquery",
]);

export function CreateProjectForm({
  guardBeforeCreate,
  onCreated,
}: {
  guardBeforeCreate: (actionName: string) => Promise<boolean>;
  onCreated: (p: DesignProject) => void;
}) {
  const t = useTranslations("workbench.bottomPanel.createProject");
  const tCommon = useTranslations("common");

  // Source-type + connection inputs
  const [sourceType, setSourceType] = useState<GenerateSourceType>("postgresql");
  const [sampleData, setSampleData] = useState("");
  const [connectionString, setConnectionString] = useState("");
  const [schemaName, setSchemaName] = useState("public");
  const [repoPath, setRepoPath] = useState("");
  const [repoUrl, setRepoUrl] = useState("");
  const [title, setTitle] = useState("");
  const [duckdbFilePath, setDuckdbFilePath] = useState("");
  const [mysqlDatabase, setMysqlDatabase] = useState("");
  const [mongoDatabase, setMongoDatabase] = useState("");
  const [sfAccount, setSfAccount] = useState("");
  const [sfUser, setSfUser] = useState("");
  const [sfPassword, setSfPassword] = useState("");
  const [sfWarehouse, setSfWarehouse] = useState("");
  const [sfDatabase, setSfDatabase] = useState("");
  const [sfSchema, setSfSchema] = useState("PUBLIC");
  const [bqProjectId, setBqProjectId] = useState("");
  const [bqDataset, setBqDataset] = useState("");
  const [bqBillingProjectId, setBqBillingProjectId] = useState("");
  const [bqCredentialsPath, setBqCredentialsPath] = useState("");

  // Phase machine
  const [phase, setPhase] = useState<Phase>("connection");
  const [previewedSource, setPreviewedSource] = useState<DesignSource | null>(
    null,
  );
  const [selectedTables, setSelectedTables] = useState<Set<string>>(new Set());

  // Submission spinner
  const [loading, setLoading] = useState(false);

  // Field-level "touched" tracking
  const [touched, setTouched] = useState<Record<string, boolean>>({});
  const markTouched = useCallback(
    (field: string) => setTouched((prev) => ({ ...prev, [field]: true })),
    [],
  );

  const isDbSource = DB_SOURCE_TYPES.has(sourceType);

  // Whenever the user edits any source input we must drop the cached
  // preview — the connection details no longer match. Forcing the
  // user back to the connection phase keeps the model coherent.
  const resetPreview = useCallback(() => {
    if (previewedSource !== null) setPreviewedSource(null);
    if (selectedTables.size > 0) setSelectedTables(new Set());
    if (phase !== "connection") setPhase("connection");
  }, [previewedSource, selectedTables.size, phase]);

  // ---------------------------------------------------------------------
  // Validation
  // ---------------------------------------------------------------------

  const connectionError =
    touched.connectionString && isDbSource && sourceType !== "bigquery" && sourceType !== "snowflake" && !connectionString.trim()
      ? t("connectionStringRequired")
      : undefined;
  const mysqlDatabaseError =
    touched.mysqlDatabase && sourceType === "mysql" && !mysqlDatabase.trim()
      ? t("databaseRequired")
      : undefined;
  const mongoDatabaseError =
    touched.mongoDatabase && sourceType === "mongodb" && !mongoDatabase.trim()
      ? t("databaseRequired")
      : undefined;
  const sfAccountError =
    touched.sfAccount && sourceType === "snowflake" && !sfAccount.trim()
      ? t("sfAccountRequired")
      : undefined;
  const sfUserError =
    touched.sfUser && sourceType === "snowflake" && !sfUser.trim()
      ? t("sfUserRequired")
      : undefined;
  const sfPasswordError =
    touched.sfPassword && sourceType === "snowflake" && !sfPassword.trim()
      ? t("sfPasswordRequired")
      : undefined;
  const sfDatabaseError =
    touched.sfDatabase && sourceType === "snowflake" && !sfDatabase.trim()
      ? t("sfDatabaseRequired")
      : undefined;
  const bqProjectIdError =
    touched.bqProjectId && sourceType === "bigquery" && !bqProjectId.trim()
      ? t("bqProjectIdRequired")
      : undefined;
  const bqDatasetError =
    touched.bqDataset && sourceType === "bigquery" && !bqDataset.trim()
      ? t("bqDatasetRequired")
      : undefined;
  const duckdbFilePathError =
    touched.duckdbFilePath && sourceType === "duckdb" && !duckdbFilePath.trim()
      ? t("duckdbFileRequired")
      : undefined;
  const repoUrlError =
    touched.repoUrl && sourceType === "code_repository" && !repoUrl.trim()
      ? t("repoUrlRequired")
      : undefined;
  const sampleDataError =
    touched.sampleData &&
    !isDbSource &&
    sourceType !== "code_repository" &&
    sourceType !== "duckdb" &&
    !sampleData.trim()
      ? sourceType === "text"
        ? t("sampleDataRequired")
        : t("sourceDataRequired")
      : undefined;

  // ---------------------------------------------------------------------
  // Source builder
  // ---------------------------------------------------------------------

  const buildSource = useCallback((): DesignSource | null => {
    switch (sourceType) {
      case "postgresql":
        if (!connectionString.trim()) return null;
        return {
          type: "postgresql",
          connection_string: connectionString.trim(),
          schema: schemaName.trim() || "public",
        };
      case "mysql":
        if (!connectionString.trim() || !mysqlDatabase.trim()) return null;
        return {
          type: "mysql",
          connection_string: connectionString.trim(),
          schema: mysqlDatabase.trim(),
        };
      case "mongodb":
        if (!connectionString.trim() || !mongoDatabase.trim()) return null;
        return {
          type: "mongodb",
          connection_string: connectionString.trim(),
          database: mongoDatabase.trim(),
        };
      case "snowflake":
        if (
          !sfAccount.trim() ||
          !sfUser.trim() ||
          !sfPassword.trim() ||
          !sfDatabase.trim()
        )
          return null;
        return {
          type: "snowflake",
          account: sfAccount.trim(),
          user: sfUser.trim(),
          password: sfPassword.trim(),
          warehouse: sfWarehouse.trim(),
          database: sfDatabase.trim(),
          schema: sfSchema.trim() || "PUBLIC",
        };
      case "bigquery":
        if (!bqProjectId.trim() || !bqDataset.trim()) return null;
        return {
          type: "bigquery",
          project_id: bqProjectId.trim(),
          dataset: bqDataset.trim(),
          billing_project_id: bqBillingProjectId.trim() || undefined,
          credentials_path: bqCredentialsPath.trim() || undefined,
        };
      case "duckdb":
        if (!duckdbFilePath.trim()) return null;
        return { type: "duckdb", file_path: duckdbFilePath.trim() };
      case "code_repository":
        if (!repoUrl.trim()) return null;
        return { type: "code_repository", url: repoUrl.trim() };
      case "text":
      case "csv":
      case "json": {
        if (!sampleData.trim()) return null;
        return { type: sourceType, data: sampleData };
      }
    }
  }, [
    sourceType,
    connectionString,
    schemaName,
    mysqlDatabase,
    mongoDatabase,
    sfAccount,
    sfUser,
    sfPassword,
    sfWarehouse,
    sfDatabase,
    sfSchema,
    bqProjectId,
    bqDataset,
    bqBillingProjectId,
    bqCredentialsPath,
    duckdbFilePath,
    repoUrl,
    sampleData,
  ]);

  const canBuildSource = buildSource() !== null;

  // ---------------------------------------------------------------------
  // Preview hook
  // ---------------------------------------------------------------------

  const preview = useSourcePreview(previewedSource);
  const previewTables = useMemo(
    () => preview.data?.tables ?? [],
    [preview.data?.tables],
  );

  function handlePreview() {
    const source = buildSource();
    if (!source) return;
    setPreviewedSource(source);
    setPhase("select_tables");
  }

  function handleBackToConnection() {
    setPhase("connection");
  }

  // ---------------------------------------------------------------------
  // Submit
  // ---------------------------------------------------------------------

  async function handleCreate() {
    setTouched({
      connectionString: true,
      sampleData: true,
      repoUrl: true,
      mysqlDatabase: true,
      mongoDatabase: true,
      sfAccount: true,
      sfUser: true,
      sfPassword: true,
      sfDatabase: true,
      bqProjectId: true,
      bqDataset: true,
      duckdbFilePath: true,
    });
    const source = isDbSource ? previewedSource : buildSource();
    if (!source) return;
    if (!(await guardBeforeCreate(t("createButton")))) return;

    const selection: AnalyzeSelection = isDbSource
      ? { kind: "subset", tables: Array.from(selectedTables).sort() }
      : { kind: "all" };

    setLoading(true);
    try {
      const project = await createProject({
        title: title.trim() || undefined,
        origin_type: "source",
        source,
        repo_source: repoPath.trim()
          ? isGitUrl(repoPath.trim())
            ? { type: "git_url" as const, url: repoPath.trim() }
            : { type: "local" as const, path: repoPath.trim() }
          : undefined,
        selection,
      });
      onCreated(project);
      toast.success(t("createSuccess"), {
        description: t("createSuccessDescription", {
          status: project.status,
          revision: project.revision,
        }),
      });
    } catch (err) {
      toast.error(t("toast.createFailed"), {
        description:
          err instanceof Error ? err.message : t("toast.unknownError"),
      });
    } finally {
      setLoading(false);
    }
  }

  // ---------------------------------------------------------------------
  // Render
  // ---------------------------------------------------------------------

  const dataPlaceholder =
    sourceType === "csv"
      ? t("dataPlaceholderCsv")
      : sourceType === "json"
        ? t("dataPlaceholderJson")
        : t("dataPlaceholderText");

  if (phase === "select_tables" && previewedSource) {
    return (
      <div>
        <h2 className="mb-1 text-xs font-semibold uppercase tracking-wider text-foreground-muted">
          {t("selectTablesHeading")}
        </h2>
        <p className="mb-3 text-xs text-foreground-muted">
          {t("selectTablesIntro")}
        </p>

        {preview.isLoading ? (
          <div className="flex items-center gap-2 px-3 py-6 text-xs text-foreground-muted">
            <Spinner size="xs" />
            <span>{t("previewLoading")}</span>
          </div>
        ) : preview.isError ? (
          <div className="flex flex-col gap-2 rounded-md border border-danger-border/30 bg-danger-solid/10 px-3 py-2 text-xs text-danger-foreground">
            <div className="flex items-center gap-1.5">
              <HugeiconsIcon
                icon={CancelCircleIcon}
                className="h-3.5 w-3.5 shrink-0"
                size="100%"
              />
              <span>{t("previewFailed")}</span>
            </div>
            <span className="text-danger-foreground">
              {preview.error instanceof Error
                ? preview.error.message
                : String(preview.error ?? "")}
            </span>
          </div>
        ) : (
          <div className="max-w-2xl">
            <TableSelector
              tables={previewTables}
              selected={selectedTables}
              onChange={setSelectedTables}
              disabled={loading}
            />
          </div>
        )}

        {loading && (
          <CreateProgressBanner
            sourceType={previewedSource?.type ?? null}
            tableCount={selectedTables.size}
          />
        )}

        <div className="mt-4 flex max-w-2xl items-center gap-2">
          <Button
            variant="outline"
            size="sm"
            onClick={handleBackToConnection}
            disabled={loading}
          >
            <HugeiconsIcon
              icon={ArrowLeft01Icon}
              className="me-1 h-4 w-4"
              size="100%"
            />
            {t("backToConnection")}
          </Button>
          <div className="flex-1" />
          <Button
            onClick={handleCreate}
            disabled={loading || selectedTables.size === 0}
          >
            {loading ? (
              <Spinner size="xs" className="me-2" />
            ) : (
              <HugeiconsIcon
                icon={PlusSignIcon}
                className="me-2 h-4 w-4"
                size="100%"
              />
            )}
            {loading ? tCommon("creating") : t("createButton")}
          </Button>
        </div>
        {selectedTables.size === 0 && !preview.isLoading && !preview.isError && (
          <p className="mt-2 max-w-2xl text-end text-2xs text-foreground-muted">
            {t("noTablesSelected")}
          </p>
        )}
      </div>
    );
  }

  return (
    <div>
      <h2 className="mb-1 text-xs font-semibold uppercase tracking-wider text-foreground-muted">
        {t("heading")}
      </h2>
      <p className="mb-3 text-xs text-foreground-muted">
        {t.rich("intro", {
          asterisk: (chunks) => <span className="text-danger-foreground">{chunks}</span>,
        })}
      </p>

      <div className="grid max-w-2xl grid-cols-2 gap-3">
        <FormField label={t("titleLabel")} hint={t("titleHint")}>
          <FormInput
            type="text"
            placeholder={t("titlePlaceholder")}
            value={title}
            onChange={(e) => setTitle(e.target.value)}
          />
        </FormField>

        <FormField label={t("sourceTypeLabel")} required>
          <FormSelect
            value={sourceType}
            onChange={(e) => {
              setSourceType(e.target.value as GenerateSourceType);
              resetPreview();
            }}
            density="settings"
          >
            <option value="postgresql">{t("sourceTypes.postgresql")}</option>
            <option value="mysql">{t("sourceTypes.mysql")}</option>
            <option value="mongodb">{t("sourceTypes.mongodb")}</option>
            <option value="snowflake">{t("sourceTypes.snowflake")}</option>
            <option value="bigquery">{t("sourceTypes.bigquery")}</option>
            <option value="duckdb">{t("sourceTypes.duckdb")}</option>
            <option value="csv">{t("sourceTypes.csv")}</option>
            <option value="json">{t("sourceTypes.json")}</option>
            <option value="code_repository">
              {t("sourceTypes.codeRepository")}
            </option>
            <option value="text">{t("sourceTypes.text")}</option>
          </FormSelect>
        </FormField>

        {sourceType === "postgresql" ? (
          <>
            <FormField
              label={t("connectionStringLabel")}
              required
              error={connectionError}
              hint={t("postgresHint")}
            >
              <SecretInput
                placeholder={t("postgresHint")}
                value={connectionString}
                onChange={(e) => {
                  setConnectionString(e.target.value);
                  resetPreview();
                }}
                onBlur={() => markTouched("connectionString")}
                error={!!connectionError}
              />
            </FormField>
            <FormField label={t("schemaLabel")} hint={t("schemaHint")}>
              <FormInput
                type="text"
                placeholder={t("schemaPlaceholder")}
                value={schemaName}
                onChange={(e) => {
                  setSchemaName(e.target.value);
                  resetPreview();
                }}
              />
            </FormField>
          </>
        ) : sourceType === "mysql" ? (
          <>
            <FormField
              label={t("connectionStringLabel")}
              required
              error={connectionError}
              hint={t("mysqlHint")}
            >
              <SecretInput
                placeholder={t("mysqlHint")}
                value={connectionString}
                onChange={(e) => {
                  setConnectionString(e.target.value);
                  resetPreview();
                }}
                onBlur={() => markTouched("connectionString")}
                error={!!connectionError}
              />
            </FormField>
            <FormField
              label={t("databaseLabel")}
              required
              error={mysqlDatabaseError}
            >
              <FormInput
                type="text"
                placeholder={t("databasePlaceholder")}
                value={mysqlDatabase}
                onChange={(e) => {
                  setMysqlDatabase(e.target.value);
                  resetPreview();
                }}
                onBlur={() => markTouched("mysqlDatabase")}
                error={!!mysqlDatabaseError}
              />
            </FormField>
          </>
        ) : sourceType === "mongodb" ? (
          <>
            <FormField
              label={t("connectionStringLabel")}
              required
              error={connectionError}
              hint={t("mongoHint")}
            >
              <SecretInput
                // i18n-audit-ignore — connection-string format example, language-neutral
                placeholder="mongodb://user:password@host:27017"
                value={connectionString}
                onChange={(e) => {
                  setConnectionString(e.target.value);
                  resetPreview();
                }}
                onBlur={() => markTouched("connectionString")}
                error={!!connectionError}
              />
            </FormField>
            <FormField
              label={t("databaseLabel")}
              required
              error={mongoDatabaseError}
            >
              <FormInput
                type="text"
                placeholder={t("databasePlaceholder")}
                value={mongoDatabase}
                onChange={(e) => {
                  setMongoDatabase(e.target.value);
                  resetPreview();
                }}
                onBlur={() => markTouched("mongoDatabase")}
                error={!!mongoDatabaseError}
              />
            </FormField>
          </>
        ) : sourceType === "snowflake" ? (
          <>
            <FormField
              label={t("sfAccountLabel")}
              required
              error={sfAccountError}
              hint={t("sfAccountHint")}
            >
              <FormInput
                type="text"
                placeholder={t("sfAccountPlaceholder")}
                value={sfAccount}
                onChange={(e) => {
                  setSfAccount(e.target.value);
                  resetPreview();
                }}
                onBlur={() => markTouched("sfAccount")}
                error={!!sfAccountError}
                className="font-mono"
              />
            </FormField>
            <FormField label={t("sfUserLabel")} required error={sfUserError}>
              <FormInput
                type="text"
                placeholder={t("sfUserPlaceholder")}
                value={sfUser}
                onChange={(e) => {
                  setSfUser(e.target.value);
                  resetPreview();
                }}
                onBlur={() => markTouched("sfUser")}
                error={!!sfUserError}
              />
            </FormField>
            <FormField
              label={t("sfPasswordLabel")}
              required
              error={sfPasswordError}
            >
              <SecretInput
                placeholder="••••••••"
                value={sfPassword}
                onChange={(e) => {
                  setSfPassword(e.target.value);
                  resetPreview();
                }}
                onBlur={() => markTouched("sfPassword")}
                error={!!sfPasswordError}
              />
            </FormField>
            <FormField
              label={t("sfWarehouseLabel")}
              hint={t("sfWarehouseHint")}
            >
              <FormInput
                type="text"
                placeholder={t("sfWarehousePlaceholder")}
                value={sfWarehouse}
                onChange={(e) => {
                  setSfWarehouse(e.target.value);
                  resetPreview();
                }}
              />
            </FormField>
            <FormField
              label={t("sfDatabaseLabel")}
              required
              error={sfDatabaseError}
            >
              <FormInput
                type="text"
                placeholder={t("sfDatabasePlaceholder")}
                value={sfDatabase}
                onChange={(e) => {
                  setSfDatabase(e.target.value);
                  resetPreview();
                }}
                onBlur={() => markTouched("sfDatabase")}
                error={!!sfDatabaseError}
              />
            </FormField>
            <FormField label={t("sfSchemaLabel")} hint={t("sfSchemaHint")}>
              <FormInput
                type="text"
                placeholder={t("sfSchemaPlaceholder")}
                value={sfSchema}
                onChange={(e) => {
                  setSfSchema(e.target.value);
                  resetPreview();
                }}
              />
            </FormField>
          </>
        ) : sourceType === "bigquery" ? (
          <>
            <FormField
              label={t("bqProjectIdLabel")}
              required
              error={bqProjectIdError}
              hint={t("bqProjectIdHint")}
            >
              <FormInput
                type="text"
                placeholder={t("bqProjectIdPlaceholder")}
                value={bqProjectId}
                onChange={(e) => {
                  setBqProjectId(e.target.value);
                  resetPreview();
                }}
                onBlur={() => markTouched("bqProjectId")}
                error={!!bqProjectIdError}
                className="font-mono"
              />
            </FormField>
            <FormField
              label={t("bqDatasetLabel")}
              required
              error={bqDatasetError}
              hint={t("bqDatasetHint")}
            >
              <FormInput
                type="text"
                placeholder={t("bqDatasetPlaceholder")}
                value={bqDataset}
                onChange={(e) => {
                  setBqDataset(e.target.value);
                  resetPreview();
                }}
                onBlur={() => markTouched("bqDataset")}
                error={!!bqDatasetError}
                className="font-mono"
              />
            </FormField>
            <div className="col-span-2">
              <FormField
                label={t("bqBillingProjectIdLabel")}
                hint={t("bqBillingProjectIdHint")}
              >
                <FormInput
                  type="text"
                  placeholder={t("bqBillingProjectIdPlaceholder")}
                  value={bqBillingProjectId}
                  onChange={(e) => {
                    setBqBillingProjectId(e.target.value);
                    resetPreview();
                  }}
                  className="font-mono"
                />
              </FormField>
            </div>
            <div className="col-span-2">
              <FormField
                label={t("bqCredentialsLabel")}
                hint={t("bqCredentialsHint")}
              >
                <FormInput
                  type="text"
                  placeholder={t("bqCredentialsPlaceholder")}
                  value={bqCredentialsPath}
                  onChange={(e) => {
                    setBqCredentialsPath(e.target.value);
                    resetPreview();
                  }}
                  className="font-mono"
                />
              </FormField>
            </div>
          </>
        ) : sourceType === "duckdb" ? (
          <div className="col-span-2">
            <FormField
              label={t("duckdbFileLabel")}
              required
              error={duckdbFilePathError}
              hint={t("duckdbFileHint")}
            >
              <FormInput
                type="text"
                placeholder={t("duckdbFilePlaceholder")}
                value={duckdbFilePath}
                onChange={(e) => setDuckdbFilePath(e.target.value)}
                onBlur={() => markTouched("duckdbFilePath")}
                error={!!duckdbFilePathError}
                className="font-mono"
              />
            </FormField>
          </div>
        ) : sourceType === "code_repository" ? (
          <div className="col-span-2">
            <FormField
              label={t("repoUrlLabel")}
              required
              error={repoUrlError}
              hint={t("repoUrlHint")}
            >
              <FormInput
                type="text"
                placeholder={t("repoUrlPlaceholder")}
                value={repoUrl}
                onChange={(e) => setRepoUrl(e.target.value)}
                onBlur={() => markTouched("repoUrl")}
                error={!!repoUrlError}
                className="font-mono"
              />
            </FormField>
          </div>
        ) : (
          <div className="col-span-2">
            <FormField
              label={
                sourceType === "text"
                  ? t("sampleDataLabel")
                  : t("sourceDataLabel")
              }
              required
              error={sampleDataError}
            >
              <FormTextarea
                rows={5}
                placeholder={dataPlaceholder}
                value={sampleData}
                onChange={(e) => setSampleData(e.target.value)}
                onBlur={() => markTouched("sampleData")}
                error={!!sampleDataError}
                className="font-mono"
              />
            </FormField>
          </div>
        )}

        {!["text", "code_repository"].includes(sourceType) && (
          <div className="col-span-2">
            <FormField label={t("repoPathLabel")} hint={t("repoPathHint")}>
              <FormInput
                type="text"
                placeholder={t("repoPathPlaceholder")}
                value={repoPath}
                onChange={(e) => setRepoPath(e.target.value)}
              />
            </FormField>
          </div>
        )}

        <div className="col-span-2">
          {isDbSource ? (
            <Button
              onClick={handlePreview}
              disabled={!canBuildSource}
              title={canBuildSource ? undefined : t("previewTablesDisabledHint")}
              className="w-full"
            >
              {t("previewTablesButton")}
            </Button>
          ) : (
            <Button
              onClick={handleCreate}
              disabled={!canBuildSource || loading}
              title={
                !canBuildSource
                  ? t("createDisabledHint")
                  : loading
                    ? undefined
                    : undefined
              }
              className="w-full"
            >
              {loading ? (
                <Spinner size="xs" className="me-2" />
              ) : (
                <HugeiconsIcon
                  icon={PlusSignIcon}
                  className="me-2 h-4 w-4"
                  size="100%"
                />
              )}
              {loading ? tCommon("creating") : t("createButton")}
            </Button>
          )}
        </div>
      </div>
    </div>
  );
}

/**
 * Friendly progress banner during project create — the analyse call
 * is server-side and indeterminate (no per-step events yet), so the
 * banner just keeps the user grounded with elapsed seconds and a
 * source-specific hint about expected duration.
 */
function CreateProgressBanner({
  sourceType,
  tableCount,
}: {
  sourceType: string | null;
  tableCount: number;
}) {
  const t = useTranslations("workbench.bottomPanel.createProject.progress");
  const [elapsed, setElapsed] = useState(0);
  const startedAt = useRef<number | null>(null);
  useEffect(() => {
    startedAt.current = Date.now();
    const id = window.setInterval(() => {
      const start = startedAt.current ?? Date.now();
      setElapsed(Math.round((Date.now() - start) / 1000));
    }, 500);
    return () => window.clearInterval(id);
  }, []);

  const isUsageBilled =
    sourceType === "bigquery" || sourceType === "snowflake";

  return (
    <div
      role="status"
      aria-live="polite"
      className="mt-3 flex max-w-2xl items-start gap-3 rounded-md border border-brand-border bg-brand-surface px-3 py-2"
    >
      <Spinner size="sm" className="mt-0.5 shrink-0 text-brand-foreground" />
      <div className="min-w-0 flex-1">
        <p className="text-xs font-medium text-brand-foreground-strong">
          {t("heading", { elapsed, count: tableCount })}
        </p>
        <p className="mt-0.5 text-xs text-brand-foreground-strong">
          {isUsageBilled ? t("hintUsageBilled") : t("hintGeneric")}
        </p>
      </div>
    </div>
  );
}
