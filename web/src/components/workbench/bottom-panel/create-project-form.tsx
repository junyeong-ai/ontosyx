"use client";

import { useState, useCallback } from "react";
import { useTranslations } from "next-intl";
import { HugeiconsIcon } from "@hugeicons/react";
import {
  PlusSignIcon,
  CheckmarkCircle01Icon,
  CancelCircleIcon,
  ArrowDown01Icon,
  ArrowUp01Icon,
} from "@hugeicons/core-free-icons";
import { createProject, testSourceConnection } from "@/lib/api";
import type { TestConnectionResponse } from "@/lib/api/sources";
import { isGitUrl } from "@/lib/git-url";
import { Button } from "@/components/ui/button";
import { FormInput } from "@/components/ui/form-input";
import { FormTextarea } from "@/components/ui/form-textarea";
import { FormField } from "@/components/ui/form-field";
import { Spinner } from "@/components/ui/spinner";
import { toast } from "sonner";
import type { DesignProject, DesignSource } from "@/types/api";
import { type GenerateSourceType, selectClassName } from "./design-panel-shared";

export function CreateProjectForm({
  guardBeforeCreate,
  onCreated,
}: {
  guardBeforeCreate: (actionName: string) => Promise<boolean>;
  onCreated: (p: DesignProject) => void;
}) {
  const t = useTranslations("workbench.bottomPanel.createProject");
  const [sourceType, setSourceType] = useState<GenerateSourceType>("postgresql");
  const [sampleData, setSampleData] = useState("");
  const [connectionString, setConnectionString] = useState("");
  const [schemaName, setSchemaName] = useState("public");
  const [repoPath, setRepoPath] = useState("");
  const [repoUrl, setRepoUrl] = useState("");
  const [title, setTitle] = useState("");
  const [loading, setLoading] = useState(false);
  const [testing, setTesting] = useState(false);
  const [testResult, setTestResult] = useState<TestConnectionResponse | null>(null);
  const [showTables, setShowTables] = useState(false);

  // DuckDB file path
  const [duckdbFilePath, setDuckdbFilePath] = useState("");

  // Ontology mode state
  // Inline validation — track which fields have been touched
  const [touched, setTouched] = useState<Record<string, boolean>>({});
  const markTouched = useCallback(
    (field: string) => setTouched((prev) => ({ ...prev, [field]: true })),
    [],
  );

  // Validation errors (only shown after field is touched)
  const [mysqlDatabase, setMysqlDatabase] = useState("");
  const [mongoDatabase, setMongoDatabase] = useState("");

  // Snowflake fields
  const [sfAccount, setSfAccount] = useState("");
  const [sfUser, setSfUser] = useState("");
  const [sfPassword, setSfPassword] = useState("");
  const [sfWarehouse, setSfWarehouse] = useState("");
  const [sfDatabase, setSfDatabase] = useState("");
  const [sfSchema, setSfSchema] = useState("PUBLIC");

  // BigQuery fields
  const [bqProjectId, setBqProjectId] = useState("");
  const [bqDataset, setBqDataset] = useState("");
  const [bqCredentialsPath, setBqCredentialsPath] = useState("");

  const isDbSource = sourceType === "postgresql" || sourceType === "mysql" || sourceType === "mongodb" || sourceType === "snowflake" || sourceType === "bigquery";

  // Reset test connection result when relevant inputs change
  const clearTestResult = useCallback(() => {
    setTestResult(null);
    setShowTables(false);
  }, []);

  const connectionError =
    touched.connectionString && isDbSource && !connectionString.trim()
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

  function buildSource(): DesignSource | null {
    if (sourceType === "postgresql") {
      if (!connectionString.trim()) return null;
      return {
        type: "postgresql",
        connection_string: connectionString.trim(),
        schema: schemaName.trim() || "public",
      };
    }
    if (sourceType === "mysql") {
      if (!connectionString.trim() || !mysqlDatabase.trim()) return null;
      return {
        type: "mysql",
        connection_string: connectionString.trim(),
        schema: mysqlDatabase.trim(),
      };
    }
    if (sourceType === "mongodb") {
      if (!connectionString.trim() || !mongoDatabase.trim()) return null;
      return {
        type: "mongodb",
        connection_string: connectionString.trim(),
        database: mongoDatabase.trim(),
      };
    }
    if (sourceType === "snowflake") {
      if (!sfAccount.trim() || !sfUser.trim() || !sfPassword.trim() || !sfDatabase.trim()) return null;
      return {
        type: "snowflake",
        account: sfAccount.trim(),
        user: sfUser.trim(),
        password: sfPassword.trim(),
        warehouse: sfWarehouse.trim(),
        database: sfDatabase.trim(),
        schema: sfSchema.trim() || "PUBLIC",
      };
    }
    if (sourceType === "bigquery") {
      if (!bqProjectId.trim() || !bqDataset.trim()) return null;
      return {
        type: "bigquery",
        project_id: bqProjectId.trim(),
        dataset: bqDataset.trim(),
        credentials_path: bqCredentialsPath.trim() || undefined,
      };
    }
    if (sourceType === "duckdb") {
      if (!duckdbFilePath.trim()) return null;
      return { type: "duckdb", file_path: duckdbFilePath.trim() };
    }
    if (sourceType === "code_repository") {
      if (!repoUrl.trim()) return null;
      return { type: "code_repository", url: repoUrl.trim() };
    }
    if (!sampleData.trim()) return null;
    return sourceType === "text"
      ? { type: "text", data: sampleData }
      : sourceType === "csv"
        ? { type: "csv", data: sampleData }
        : { type: "json", data: sampleData };
  }

  async function handleTestConnection() {
    // BigQuery test connection builds a bigquery:// URI from fields
    if (sourceType === "bigquery") {
      if (!bqProjectId.trim() || !bqDataset.trim()) return;
      setTesting(true);
      setTestResult(null);
      setShowTables(false);
      try {
        let connStr = `bigquery://${bqProjectId.trim()}/${bqDataset.trim()}`;
        if (bqCredentialsPath.trim()) {
          connStr += `?credentials_path=${encodeURIComponent(bqCredentialsPath.trim())}`;
        }
        const result = await testSourceConnection({
          source_type: "bigquery",
          connection_string: connStr,
          schema_name: bqDataset.trim(),
        });
        setTestResult(result);
      } catch (err) {
        setTestResult({
          success: false,
          error: err instanceof Error ? err.message : "Unknown error",
          error_type: "connection_failed",
        });
      } finally {
        setTesting(false);
      }
      return;
    }

    if (!connectionString.trim()) return;
    setTesting(true);
    setTestResult(null);
    setShowTables(false);
    try {
      const schemaParam =
        sourceType === "postgresql"
          ? schemaName.trim() || "public"
          : sourceType === "mysql"
            ? mysqlDatabase.trim()
            : sourceType === "mongodb"
              ? mongoDatabase.trim()
              : undefined;
      const result = await testSourceConnection({
        source_type: sourceType,
        connection_string: connectionString.trim(),
        schema_name: schemaParam,
      });
      setTestResult(result);
    } catch (err) {
      setTestResult({
        success: false,
        error: err instanceof Error ? err.message : "Unknown error",
        error_type: "connection_failed",
      });
    } finally {
      setTesting(false);
    }
  }

  function testErrorMessage(errorType?: string, rawError?: string): string {
    switch (errorType) {
      case "auth_failed":
        return t("testErrors.authFailed");
      case "network":
        return t("testErrors.network");
      case "not_found":
        return t("testErrors.notFound");
      case "permission":
        return t("testErrors.permission");
      default:
        return rawError ?? t("testErrors.generic");
    }
  }

  async function handleCreate() {
    setTouched({ connectionString: true, sampleData: true, repoUrl: true, mysqlDatabase: true, mongoDatabase: true, sfAccount: true, sfUser: true, sfPassword: true, sfDatabase: true, bqProjectId: true, bqDataset: true, duckdbFilePath: true });
    const source = buildSource();
    if (!source) return;
    if (!(await guardBeforeCreate(t("createButton")))) return;

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
      });
      onCreated(project);
      toast.success(t("createSuccess"), {
        description: t("createSuccessDescription", { status: project.status, revision: project.revision }),
      });
    } catch (err) {
      toast.error(t("toast.createFailed"), {
        description: err instanceof Error ? err.message : t("toast.unknownError"),
      });
    } finally {
      setLoading(false);
    }
  }

  const canSubmitSource =
    !loading &&
    (sourceType === "postgresql"
      ? !!connectionString.trim()
      : sourceType === "mysql"
        ? !!connectionString.trim() && !!mysqlDatabase.trim()
        : sourceType === "mongodb"
          ? !!connectionString.trim() && !!mongoDatabase.trim()
          : sourceType === "snowflake"
            ? !!sfAccount.trim() && !!sfUser.trim() && !!sfPassword.trim() && !!sfDatabase.trim()
            : sourceType === "bigquery"
              ? !!bqProjectId.trim() && !!bqDataset.trim()
              : sourceType === "duckdb"
                ? !!duckdbFilePath.trim()
                : sourceType === "code_repository"
                  ? !!repoUrl.trim()
                  : !!sampleData.trim());

  const canSubmit = canSubmitSource;

  const dataPlaceholder =
    sourceType === "csv"
      ? t("dataPlaceholderCsv")
      : sourceType === "json"
        ? t("dataPlaceholderJson")
        : t("dataPlaceholderText");

  // Whether the test connection button should check connectionString or BQ-specific fields
  const canTestConnection = sourceType === "bigquery"
    ? !!bqProjectId.trim() && !!bqDataset.trim()
    : !!connectionString.trim();

  return (
    <div>
      <h3 className="mb-1 text-xs font-semibold uppercase tracking-wider text-zinc-600 dark:text-muted-foreground">
        {t("heading")}
      </h3>
      <p className="mb-3 text-xs text-zinc-500 dark:text-muted-foreground">
        {t.rich("intro", {
          asterisk: (chunks) => <span className="text-red-500">{chunks}</span>,
        })}
      </p>

      {/* Mode toggle removed — "From Existing Ontology" is now a Fork action on completed projects in the header dropdown */}

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
              <select
                value={sourceType}
                onChange={(e) => { setSourceType(e.target.value as GenerateSourceType); clearTestResult(); }}
                className={selectClassName}
              >
                <option value="postgresql">{t("sourceTypes.postgresql")}</option>
                <option value="mysql">{t("sourceTypes.mysql")}</option>
                <option value="mongodb">{t("sourceTypes.mongodb")}</option>
                <option value="snowflake">{t("sourceTypes.snowflake")}</option>
                <option value="bigquery">{t("sourceTypes.bigquery")}</option>
                <option value="duckdb">{t("sourceTypes.duckdb")}</option>
                <option value="csv">{t("sourceTypes.csv")}</option>
                <option value="json">{t("sourceTypes.json")}</option>
                <option value="code_repository">{t("sourceTypes.codeRepository")}</option>
                <option value="text">{t("sourceTypes.text")}</option>
              </select>
            </FormField>

            {sourceType === "postgresql" ? (
              <>
                <FormField
                  label={t("connectionStringLabel")}
                  required
                  error={connectionError}
                  hint={t("postgresHint")}
                >
                  <FormInput
                    type="text"
                    placeholder={t("postgresHint")}
                    value={connectionString}
                    onChange={(e) => { setConnectionString(e.target.value); clearTestResult(); }}
                    onBlur={() => markTouched("connectionString")}
                    error={!!connectionError}
                    className="font-mono"
                  />
                </FormField>
                <FormField label={t("schemaLabel")} hint={t("schemaHint")}>
                  <FormInput
                    type="text"
                    placeholder={t("schemaPlaceholder")}
                    value={schemaName}
                    onChange={(e) => { setSchemaName(e.target.value); clearTestResult(); }}
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
                  <FormInput
                    type="text"
                    placeholder={t("mysqlHint")}
                    value={connectionString}
                    onChange={(e) => { setConnectionString(e.target.value); clearTestResult(); }}
                    onBlur={() => markTouched("connectionString")}
                    error={!!connectionError}
                    className="font-mono"
                  />
                </FormField>
                <FormField label={t("databaseLabel")} required error={mysqlDatabaseError}>
                  <FormInput
                    type="text"
                    placeholder={t("databasePlaceholder")}
                    value={mysqlDatabase}
                    onChange={(e) => { setMysqlDatabase(e.target.value); clearTestResult(); }}
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
                  <FormInput
                    type="text"
                    placeholder="mongodb://user:password@host:27017"
                    value={connectionString}
                    onChange={(e) => { setConnectionString(e.target.value); clearTestResult(); }}
                    onBlur={() => markTouched("connectionString")}
                    error={!!connectionError}
                    className="font-mono"
                  />
                </FormField>
                <FormField label={t("databaseLabel")} required error={mongoDatabaseError}>
                  <FormInput
                    type="text"
                    placeholder={t("databasePlaceholder")}
                    value={mongoDatabase}
                    onChange={(e) => { setMongoDatabase(e.target.value); clearTestResult(); }}
                    onBlur={() => markTouched("mongoDatabase")}
                    error={!!mongoDatabaseError}
                  />
                </FormField>
              </>
            ) : sourceType === "snowflake" ? (
              <>
                <FormField label={t("sfAccountLabel")} required error={sfAccountError} hint={t("sfAccountHint")}>
                  <FormInput
                    type="text"
                    placeholder={t("sfAccountPlaceholder")}
                    value={sfAccount}
                    onChange={(e) => { setSfAccount(e.target.value); clearTestResult(); }}
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
                    onChange={(e) => { setSfUser(e.target.value); clearTestResult(); }}
                    onBlur={() => markTouched("sfUser")}
                    error={!!sfUserError}
                  />
                </FormField>
                <FormField label={t("sfPasswordLabel")} required error={sfPasswordError}>
                  <FormInput
                    type="password"
                    placeholder="••••••••"
                    value={sfPassword}
                    onChange={(e) => { setSfPassword(e.target.value); clearTestResult(); }}
                    onBlur={() => markTouched("sfPassword")}
                    error={!!sfPasswordError}
                  />
                </FormField>
                <FormField label={t("sfWarehouseLabel")} hint={t("sfWarehouseHint")}>
                  <FormInput
                    type="text"
                    placeholder={t("sfWarehousePlaceholder")}
                    value={sfWarehouse}
                    onChange={(e) => { setSfWarehouse(e.target.value); clearTestResult(); }}
                  />
                </FormField>
                <FormField label={t("sfDatabaseLabel")} required error={sfDatabaseError}>
                  <FormInput
                    type="text"
                    placeholder={t("sfDatabasePlaceholder")}
                    value={sfDatabase}
                    onChange={(e) => { setSfDatabase(e.target.value); clearTestResult(); }}
                    onBlur={() => markTouched("sfDatabase")}
                    error={!!sfDatabaseError}
                  />
                </FormField>
                <FormField label={t("sfSchemaLabel")} hint={t("sfSchemaHint")}>
                  <FormInput
                    type="text"
                    placeholder={t("sfSchemaPlaceholder")}
                    value={sfSchema}
                    onChange={(e) => { setSfSchema(e.target.value); clearTestResult(); }}
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
                    onChange={(e) => { setBqProjectId(e.target.value); clearTestResult(); }}
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
                    onChange={(e) => { setBqDataset(e.target.value); clearTestResult(); }}
                    onBlur={() => markTouched("bqDataset")}
                    error={!!bqDatasetError}
                    className="font-mono"
                  />
                </FormField>
                <div className="col-span-2">
                  <FormField
                    label={t("bqCredentialsLabel")}
                    hint={t("bqCredentialsHint")}
                  >
                    <FormInput
                      type="text"
                      placeholder={t("bqCredentialsPlaceholder")}
                      value={bqCredentialsPath}
                      onChange={(e) => { setBqCredentialsPath(e.target.value); clearTestResult(); }}
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
                  label={sourceType === "text" ? t("sampleDataLabel") : t("sourceDataLabel")}
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

            {isDbSource && (
              <div className="col-span-2 flex flex-col gap-2">
                <Button
                  variant="outline"
                  size="sm"
                  onClick={handleTestConnection}
                  disabled={testing || !canTestConnection}
                  className="w-fit"
                >
                  {testing ? (
                    <Spinner size="xs" className="mr-2" />
                  ) : null}
                  {testing ? t("testing") : t("testConnectionLabel")}
                </Button>

                {testResult && (
                  <div
                    className={`rounded-md border px-3 py-2 text-xs ${
                      testResult.success
                        ? "border-emerald-500/30 bg-emerald-500/10 text-emerald-400"
                        : "border-red-500/30 bg-red-500/10 text-red-400"
                    }`}
                  >
                    <div className="flex items-center gap-1.5">
                      <HugeiconsIcon
                        icon={testResult.success ? CheckmarkCircle01Icon : CancelCircleIcon}
                        className="h-3.5 w-3.5 shrink-0"
                        size="100%"
                      />
                      <span>
                        {testResult.success
                          ? t("tablesFound", { count: testResult.table_count ?? 0 })
                          : testErrorMessage(testResult.error_type, testResult.error)}
                      </span>
                    </div>
                    {testResult.success && testResult.tables && testResult.tables.length > 0 && (
                      <div className="mt-1.5">
                        <button
                          type="button"
                          onClick={() => setShowTables(!showTables)}
                          className="flex items-center gap-1 text-xs text-emerald-400/80 hover:text-emerald-300 transition-colors"
                        >
                          <HugeiconsIcon
                            icon={showTables ? ArrowUp01Icon : ArrowDown01Icon}
                            className="h-3 w-3"
                            size="100%"
                          />
                          {showTables ? t("hideTables") : t("showTables")}
                        </button>
                        {showTables && (
                          <ul className="mt-1 max-h-32 overflow-y-auto space-y-0.5 pl-1 font-mono text-[11px] text-emerald-400/70">
                            {testResult.tables.map((tbl) => (
                              <li key={tbl}>{tbl}</li>
                            ))}
                          </ul>
                        )}
                      </div>
                    )}
                  </div>
                )}
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
          <Button onClick={handleCreate} disabled={!canSubmit} className="w-full">
            {loading ? (
              <Spinner size="xs" className="mr-2" />
            ) : (
              <HugeiconsIcon icon={PlusSignIcon} className="mr-2 h-4 w-4" size="100%" />
            )}
            {loading ? t("creating") : t("createButton")}
          </Button>
        </div>
      </div>
    </div>
  );
}
