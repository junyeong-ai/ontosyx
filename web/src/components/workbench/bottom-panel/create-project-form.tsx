"use client";

import { useState, useCallback } from "react";
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
      ? "Connection string is required"
      : undefined;
  const mysqlDatabaseError =
    touched.mysqlDatabase && sourceType === "mysql" && !mysqlDatabase.trim()
      ? "Database name is required"
      : undefined;
  const mongoDatabaseError =
    touched.mongoDatabase && sourceType === "mongodb" && !mongoDatabase.trim()
      ? "Database name is required"
      : undefined;
  const sfAccountError =
    touched.sfAccount && sourceType === "snowflake" && !sfAccount.trim()
      ? "Account identifier is required"
      : undefined;
  const sfUserError =
    touched.sfUser && sourceType === "snowflake" && !sfUser.trim()
      ? "User is required"
      : undefined;
  const sfPasswordError =
    touched.sfPassword && sourceType === "snowflake" && !sfPassword.trim()
      ? "Password is required"
      : undefined;
  const sfDatabaseError =
    touched.sfDatabase && sourceType === "snowflake" && !sfDatabase.trim()
      ? "Database is required"
      : undefined;
  const bqProjectIdError =
    touched.bqProjectId && sourceType === "bigquery" && !bqProjectId.trim()
      ? "Project ID is required"
      : undefined;
  const bqDatasetError =
    touched.bqDataset && sourceType === "bigquery" && !bqDataset.trim()
      ? "Dataset is required"
      : undefined;
  const duckdbFilePathError =
    touched.duckdbFilePath && sourceType === "duckdb" && !duckdbFilePath.trim()
      ? "File path is required"
      : undefined;
  const repoUrlError =
    touched.repoUrl && sourceType === "code_repository" && !repoUrl.trim()
      ? "Repository URL is required"
      : undefined;
  const sampleDataError =
    touched.sampleData &&
    !isDbSource &&
    sourceType !== "code_repository" &&
    sourceType !== "duckdb" &&
    !sampleData.trim()
      ? `${sourceType === "text" ? "Sample data" : "Source data"} is required`
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
        return "Authentication failed. Check username and password.";
      case "network":
        return "Could not connect. Check host and port.";
      case "not_found":
        return "Database not found. Check the database name.";
      case "permission":
        return "Access denied. Check user permissions.";
      default:
        return rawError ?? "Connection failed.";
    }
  }

  async function handleCreate() {
    setTouched({ connectionString: true, sampleData: true, repoUrl: true, mysqlDatabase: true, mongoDatabase: true, sfAccount: true, sfUser: true, sfPassword: true, sfDatabase: true, bqProjectId: true, bqDataset: true, duckdbFilePath: true });
    const source = buildSource();
    if (!source) return;
    if (!(await guardBeforeCreate("Create Project"))) return;

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
      toast.success("Project created", {
        description: `Status: ${project.status} (rev ${project.revision})`,
      });
    } catch (err) {
      toast.error("Failed to create project", {
        description: err instanceof Error ? err.message : "Unknown error",
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
      ? "name,age,company\nAlice,30,Acme Corp\nBob,25,Globex"
      : sourceType === "json"
        ? '[{"name":"Alice","team":"Platform"},{"name":"Bob","team":"Data"}]'
        : "Describe your entities, relationships, and example records";

  // Whether the test connection button should check connectionString or BQ-specific fields
  const canTestConnection = sourceType === "bigquery"
    ? !!bqProjectId.trim() && !!bqDataset.trim()
    : !!connectionString.trim();

  return (
    <div>
      <h3 className="mb-1 text-xs font-semibold uppercase tracking-wider text-zinc-500">
        New Design Project
      </h3>
      <p className="mb-3 text-xs text-zinc-400">
        Create a project to analyze your data source and design an ontology.
        Fields marked with <span className="text-red-500">*</span> are required.
      </p>

      {/* Mode toggle removed — "From Existing Ontology" is now a Fork action on completed projects in the header dropdown */}

      <div className="grid max-w-2xl grid-cols-2 gap-3">
        <FormField label="Title" hint="Optional — auto-generated if empty">
          <FormInput
            type="text"
            placeholder="e.g. Olive Young DB Ontology"
            value={title}
            onChange={(e) => setTitle(e.target.value)}
          />
        </FormField>

        <FormField label="Source Type" required>
              <select
                value={sourceType}
                onChange={(e) => { setSourceType(e.target.value as GenerateSourceType); clearTestResult(); }}
                className={selectClassName}
              >
                <option value="postgresql">PostgreSQL</option>
                <option value="mysql">MySQL</option>
                <option value="mongodb">MongoDB</option>
                <option value="snowflake">Snowflake</option>
                <option value="bigquery">BigQuery</option>
                <option value="duckdb">Local File (DuckDB)</option>
                <option value="csv">CSV</option>
                <option value="json">JSON</option>
                <option value="code_repository">Code Repository</option>
                <option value="text">Plain Text (manual)</option>
              </select>
            </FormField>

            {sourceType === "postgresql" ? (
              <>
                <FormField
                  label="Connection String"
                  required
                  error={connectionError}
                  hint="postgres://user:password@host:5432/dbname"
                >
                  <FormInput
                    type="text"
                    placeholder="postgres://user:password@host:5432/dbname"
                    value={connectionString}
                    onChange={(e) => { setConnectionString(e.target.value); clearTestResult(); }}
                    onBlur={() => markTouched("connectionString")}
                    error={!!connectionError}
                    className="font-mono"
                  />
                </FormField>
                <FormField label="Schema" hint="Defaults to 'public'">
                  <FormInput
                    type="text"
                    placeholder="public"
                    value={schemaName}
                    onChange={(e) => { setSchemaName(e.target.value); clearTestResult(); }}
                  />
                </FormField>
              </>
            ) : sourceType === "mysql" ? (
              <>
                <FormField
                  label="Connection String"
                  required
                  error={connectionError}
                  hint="mysql://user:password@host:3306/dbname"
                >
                  <FormInput
                    type="text"
                    placeholder="mysql://user:password@host:3306/dbname"
                    value={connectionString}
                    onChange={(e) => { setConnectionString(e.target.value); clearTestResult(); }}
                    onBlur={() => markTouched("connectionString")}
                    error={!!connectionError}
                    className="font-mono"
                  />
                </FormField>
                <FormField label="Database" required error={mysqlDatabaseError}>
                  <FormInput
                    type="text"
                    placeholder="my_database"
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
                  label="Connection String"
                  required
                  error={connectionError}
                  hint="mongodb://user:password@host:27017 or mongodb+srv://..."
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
                <FormField label="Database" required error={mongoDatabaseError}>
                  <FormInput
                    type="text"
                    placeholder="my_database"
                    value={mongoDatabase}
                    onChange={(e) => { setMongoDatabase(e.target.value); clearTestResult(); }}
                    onBlur={() => markTouched("mongoDatabase")}
                    error={!!mongoDatabaseError}
                  />
                </FormField>
              </>
            ) : sourceType === "bigquery" ? (
              <>
                <FormField
                  label="GCP Project ID"
                  required
                  error={bqProjectIdError}
                  hint="e.g. my-gcp-project-123"
                >
                  <FormInput
                    type="text"
                    placeholder="my-gcp-project"
                    value={bqProjectId}
                    onChange={(e) => { setBqProjectId(e.target.value); clearTestResult(); }}
                    onBlur={() => markTouched("bqProjectId")}
                    error={!!bqProjectIdError}
                    className="font-mono"
                  />
                </FormField>
                <FormField
                  label="Dataset"
                  required
                  error={bqDatasetError}
                  hint="BigQuery dataset name"
                >
                  <FormInput
                    type="text"
                    placeholder="analytics_prod"
                    value={bqDataset}
                    onChange={(e) => { setBqDataset(e.target.value); clearTestResult(); }}
                    onBlur={() => markTouched("bqDataset")}
                    error={!!bqDatasetError}
                    className="font-mono"
                  />
                </FormField>
                <div className="col-span-2">
                  <FormField
                    label="Credentials File Path"
                    hint="Optional. Uses GOOGLE_APPLICATION_CREDENTIALS env var if empty."
                  >
                    <FormInput
                      type="text"
                      placeholder="/path/to/service-account.json"
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
                  label="File Path"
                  required
                  error={duckdbFilePathError}
                  hint="Absolute path to a Parquet, CSV, or JSON/JSONL file"
                >
                  <FormInput
                    type="text"
                    placeholder="/path/to/data.parquet"
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
                  label="Repository URL"
                  required
                  error={repoUrlError}
                  hint="Analyzes the default branch"
                >
                  <FormInput
                    type="text"
                    placeholder="https://github.com/org/repo"
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
                  label={sourceType === "text" ? "Sample Data" : "Source Data"}
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
                  {testing ? "Testing..." : "Test Connection"}
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
                          ? `${testResult.table_count ?? 0} table${(testResult.table_count ?? 0) === 1 ? "" : "s"} found`
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
                          {showTables ? "Hide tables" : "Show tables"}
                        </button>
                        {showTables && (
                          <ul className="mt-1 max-h-32 overflow-y-auto space-y-0.5 pl-1 font-mono text-[11px] text-emerald-400/70">
                            {testResult.tables.map((t) => (
                              <li key={t}>{t}</li>
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
                <FormField label="Repo Path or Git URL" hint="Optional — used for code analysis">
                  <FormInput
                    type="text"
                    placeholder="/path/to/repo or https://github.com/org/repo"
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
            {loading ? "Creating..." : "Create Project"}
          </Button>
        </div>
      </div>
    </div>
  );
}
