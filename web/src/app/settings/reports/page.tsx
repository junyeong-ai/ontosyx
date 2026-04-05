"use client";

import { useEffect, useState } from "react";
import { Spinner } from "@/components/ui/spinner";
import { SettingsSelect } from "@/components/ui/form-input";
import { toast } from "sonner";
import { useConfirm } from "@/components/ui/confirm-dialog";
import type {
  SavedReport,
  SavedOntology,
  ReportCreateRequest,
  ReportUpdateRequest,
  ReportParameter,
  QueryResult,
} from "@/types/api";
import {
  listReports,
  createReport,
  updateReport,
  deleteReport,
  executeReport,
  listOntologies,
} from "@/lib/api";
import { WIDGET_TYPES } from "@/components/widgets/widget-types";

export default function ReportsPage() {
  const [reports, setReports] = useState<SavedReport[]>([]);
  const [ontologies, setOntologies] = useState<SavedOntology[]>([]);
  const [loading, setLoading] = useState(true);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [ontologyFilter, setOntologyFilter] = useState<string>("");

  useEffect(() => {
    listOntologies({ limit: 100 })
      .then((page) => {
        setOntologies(page.items);
        if (page.items.length > 0) {
          setOntologyFilter(page.items[0].id);
        }
      })
      .catch(() => toast.error("Failed to load ontologies"))
      .finally(() => setLoading(false));
  }, []);

  useEffect(() => {
    if (!ontologyFilter) return;
    setLoading(true);
    listReports({ ontology_id: ontologyFilter })
      .then((page) => setReports(page.items))
      .catch(() => toast.error("Failed to load reports"))
      .finally(() => setLoading(false));
  }, [ontologyFilter]);

  const confirm = useConfirm();

  const handleDelete = async (id: string) => {
    const report = reports.find((r) => r.id === id);
    const ok = await confirm({
      title: `Delete report '${report?.title ?? id}'?`,
      description: "This action cannot be undone. The saved report will be permanently removed.",
      variant: "danger",
    });
    if (!ok) return;
    try {
      await deleteReport(id);
      setReports((prev) => prev.filter((r) => r.id !== id));
      if (selectedId === id) setSelectedId(null);
      toast.success("Report deleted");
    } catch {
      toast.error("Delete failed");
    }
  };

  const handleCreate = async (values: ReportCreateRequest) => {
    const report = await createReport(values);
    setReports((prev) => [report, ...prev]);
    toast.success("Report created");
  };

  const handleUpdate = async (id: string, patch: ReportUpdateRequest) => {
    try {
      const updated = await updateReport(id, patch);
      setReports((prev) => prev.map((r) => (r.id === id ? updated : r)));
      toast.success("Report updated");
    } catch {
      toast.error("Update failed");
    }
  };

  if (loading && ontologies.length === 0) {
    return (
      <div className="flex items-center justify-center py-12">
        <Spinner size="lg" />
      </div>
    );
  }

  const selected = reports.find((r) => r.id === selectedId);

  return (
    <div>
      <h1 className="text-lg font-semibold text-zinc-800 dark:text-zinc-200">
        Saved Reports
      </h1>
      <p className="mt-1 text-sm text-zinc-500">
        Parameterized query templates that can be executed on demand.
      </p>

      {/* Ontology filter */}
      <div className="mt-4">
        <label className="text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
          Ontology
        </label>
        <SettingsSelect
          value={ontologyFilter}
          onChange={(e) => {
            setOntologyFilter(e.target.value);
            setSelectedId(null);
          }}
          className="w-64"
        >
          {ontologies.map((o) => (
            <option key={o.id} value={o.id}>
              {o.name} (v{o.version})
            </option>
          ))}
        </SettingsSelect>
      </div>

      {ontologyFilter && (
        <ReportCreateForm
          ontologyId={ontologyFilter}
          onSubmit={handleCreate}
        />
      )}

      {loading ? (
        <div className="mt-6 flex items-center justify-center py-8">
          <Spinner size="sm" />
        </div>
      ) : (
        <div className="mt-6 flex gap-6">
          {/* Report list */}
          <div className="w-72 shrink-0 space-y-1">
            {reports.length === 0 ? (
              <p className="text-sm text-zinc-400">
                No reports for this ontology.
              </p>
            ) : (
              reports.map((r) => (
                <button
                  key={r.id}
                  onClick={() => setSelectedId(r.id)}
                  className={`w-full rounded-md px-3 py-2 text-left text-sm transition-colors ${
                    r.id === selectedId
                      ? "bg-emerald-50 text-emerald-700 dark:bg-emerald-900/20 dark:text-emerald-400"
                      : "text-zinc-700 hover:bg-zinc-50 dark:text-zinc-300 dark:hover:bg-zinc-800"
                  }`}
                >
                  <div className="flex items-center gap-2">
                    <span className="font-medium truncate">{r.title}</span>
                    <span
                      className={`shrink-0 rounded-full px-1.5 py-0.5 text-[9px] font-medium ${
                        r.is_public
                          ? "bg-emerald-100 text-emerald-700 dark:bg-emerald-900/30 dark:text-emerald-400"
                          : "bg-zinc-100 text-zinc-500 dark:bg-zinc-800 dark:text-zinc-400"
                      }`}
                    >
                      {r.is_public ? "public" : "private"}
                    </span>
                  </div>
                  <div className="text-xs text-zinc-400">
                    {r.widget_type ?? "auto"} · {r.parameters.length} params
                  </div>
                </button>
              ))
            )}
          </div>

          {/* Detail */}
          <div className="flex-1">
            {selected ? (
              <ReportDetail
                report={selected}
                onDelete={handleDelete}
                onUpdate={handleUpdate}
              />
            ) : (
              <div className="text-sm text-zinc-400">
                Select a report to view details.
              </div>
            )}
          </div>
        </div>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Report detail — inline edit + execute
// ---------------------------------------------------------------------------

function ReportDetail({
  report,
  onDelete,
  onUpdate,
}: {
  report: SavedReport;
  onDelete: (id: string) => void;
  onUpdate: (id: string, patch: ReportUpdateRequest) => void;
}) {
  const [editing, setEditing] = useState(false);
  const confirm = useConfirm();
  const [executing, setExecuting] = useState(false);
  const [paramValues, setParamValues] = useState<Record<string, unknown>>({});
  const [result, setResult] = useState<QueryResult | null>(null);

  // Edit form state
  const [editTitle, setEditTitle] = useState(report.title);
  const [editDescription, setEditDescription] = useState(report.description ?? "");
  const [editQueryTemplate, setEditQueryTemplate] = useState(report.query_template);
  const [editWidgetType, setEditWidgetType] = useState(report.widget_type ?? "");
  const [editIsPublic, setEditIsPublic] = useState(report.is_public);

  // Reset edit state when report changes
  useEffect(() => {
    setEditing(false);
    setResult(null);
    setEditTitle(report.title);
    setEditDescription(report.description ?? "");
    setEditQueryTemplate(report.query_template);
    setEditWidgetType(report.widget_type ?? "");
    setEditIsPublic(report.is_public);
    // Initialize param values with defaults
    const defaults: Record<string, unknown> = {};
    for (const p of report.parameters) {
      defaults[p.name] = p.default ?? "";
    }
    setParamValues(defaults);
  }, [report.id, report.title, report.description, report.query_template, report.widget_type, report.is_public, report.parameters]);

  const handleSaveEdit = () => {
    const patch: ReportUpdateRequest = {};
    if (editTitle !== report.title) patch.title = editTitle;
    if (editDescription !== (report.description ?? "")) patch.description = editDescription;
    if (editQueryTemplate !== report.query_template) patch.query_template = editQueryTemplate;
    if (editWidgetType !== (report.widget_type ?? "")) patch.widget_type = editWidgetType || undefined;
    if (editIsPublic !== report.is_public) patch.is_public = editIsPublic;
    onUpdate(report.id, patch);
    setEditing(false);
  };

  const handleExecute = async () => {
    setExecuting(true);
    setResult(null);
    try {
      const res = await executeReport(report.id, paramValues);
      setResult(res);
    } catch {
      toast.error("Execution failed");
    } finally {
      setExecuting(false);
    }
  };

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-sm font-semibold text-zinc-800 dark:text-zinc-200">
            {report.title}
          </h2>
          <p className="text-xs text-zinc-400">
            {new Date(report.created_at).toLocaleDateString()} · updated{" "}
            {new Date(report.updated_at).toLocaleDateString()}
          </p>
        </div>
        <div className="flex items-center gap-2">
          <button
            onClick={() => setEditing(!editing)}
            className="rounded-md px-3 py-1.5 text-xs font-medium text-zinc-600 hover:bg-zinc-50 dark:text-zinc-400 dark:hover:bg-zinc-800"
          >
            {editing ? "Cancel" : "Edit"}
          </button>
          <button
            onClick={async () => {
              const ok = await confirm({
                title: `Delete report '${report.title}'?`,
                description: "This action cannot be undone. The saved report will be permanently removed.",
                variant: "danger",
              });
              if (ok) onDelete(report.id);
            }}
            className="rounded-md px-3 py-1.5 text-xs font-medium text-red-600 hover:bg-red-50 dark:hover:bg-red-950"
          >
            Delete
          </button>
        </div>
      </div>

      {/* Inline edit form */}
      {editing ? (
        <div className="space-y-3 rounded-lg border border-emerald-200 bg-emerald-50/30 p-4 dark:border-emerald-800 dark:bg-emerald-950/10">
          <div>
            <label className="text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
              Title
            </label>
            <input
              value={editTitle}
              onChange={(e) => setEditTitle(e.target.value)}
              className="mt-0.5 w-full rounded-md border border-zinc-200 bg-white px-3 py-1.5 text-xs dark:border-zinc-700 dark:bg-zinc-900"
            />
          </div>
          <div>
            <label className="text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
              Description
            </label>
            <textarea
              value={editDescription}
              onChange={(e) => setEditDescription(e.target.value)}
              rows={2}
              className="mt-0.5 w-full rounded-md border border-zinc-200 bg-white px-3 py-1.5 text-xs dark:border-zinc-700 dark:bg-zinc-900"
            />
          </div>
          <div>
            <label className="text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
              Query Template
            </label>
            <textarea
              value={editQueryTemplate}
              onChange={(e) => setEditQueryTemplate(e.target.value)}
              rows={6}
              className="mt-0.5 w-full rounded-md border border-zinc-200 bg-white px-3 py-1.5 font-mono text-xs dark:border-zinc-700 dark:bg-zinc-900"
            />
          </div>
          <div>
            <label className="text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
              Widget Type
            </label>
            <SettingsSelect
              value={editWidgetType}
              onChange={(e) => setEditWidgetType(e.target.value)}
            >
              <option value="">Auto</option>
              {WIDGET_TYPES.map((t) => (
                <option key={t.value} value={t.value}>
                  {t.label}
                </option>
              ))}
            </SettingsSelect>
          </div>
          <div className="flex items-center gap-2">
            <label className="text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
              Public
            </label>
            <button
              type="button"
              onClick={() => setEditIsPublic(!editIsPublic)}
              className={`relative inline-flex h-5 w-9 items-center rounded-full transition-colors ${
                editIsPublic ? "bg-emerald-500" : "bg-zinc-300 dark:bg-zinc-600"
              }`}
            >
              <span
                className={`inline-block h-3.5 w-3.5 transform rounded-full bg-white transition-transform ${
                  editIsPublic ? "translate-x-4.5" : "translate-x-0.5"
                }`}
              />
            </button>
          </div>
          <button
            onClick={handleSaveEdit}
            className="rounded-md bg-emerald-600 px-4 py-1.5 text-xs font-medium text-white hover:bg-emerald-700"
          >
            Save Changes
          </button>
        </div>
      ) : (
        <>
          {report.description && (
            <div>
              <label className="text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
                Description
              </label>
              <p className="mt-0.5 text-sm text-zinc-700 dark:text-zinc-300">
                {report.description}
              </p>
            </div>
          )}

          <div>
            <label className="text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
              Query Template
            </label>
            <pre className="mt-1 max-h-48 overflow-auto rounded-md bg-zinc-900 p-3 text-xs text-emerald-400">
              {report.query_template}
            </pre>
          </div>

          {report.widget_type && (
            <div>
              <label className="text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
                Widget Type
              </label>
              <span className="ml-2 rounded bg-zinc-100 px-1.5 py-0.5 text-xs text-zinc-600 dark:bg-zinc-800 dark:text-zinc-400">
                {report.widget_type}
              </span>
            </div>
          )}

          {/* Parameters + execute */}
          <div>
            <label className="text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
              Parameters
            </label>
            {report.parameters.length === 0 ? (
              <p className="mt-0.5 text-xs text-zinc-400">No parameters.</p>
            ) : (
              <div className="mt-1 space-y-2">
                {report.parameters.map((p) => (
                  <div key={p.name} className="flex items-center gap-2">
                    <span className="w-28 shrink-0 text-xs font-medium text-zinc-600 dark:text-zinc-400">
                      {p.label || p.name}
                    </span>
                    {p.type === "boolean" ? (
                      <button
                        type="button"
                        onClick={() =>
                          setParamValues((prev) => ({
                            ...prev,
                            [p.name]: !prev[p.name],
                          }))
                        }
                        className={`relative inline-flex h-5 w-9 items-center rounded-full transition-colors ${
                          paramValues[p.name]
                            ? "bg-emerald-500"
                            : "bg-zinc-300 dark:bg-zinc-600"
                        }`}
                      >
                        <span
                          className={`inline-block h-3.5 w-3.5 transform rounded-full bg-white transition-transform ${
                            paramValues[p.name]
                              ? "translate-x-4.5"
                              : "translate-x-0.5"
                          }`}
                        />
                      </button>
                    ) : (
                      <input
                        type={p.type === "number" ? "number" : "text"}
                        value={String(paramValues[p.name] ?? "")}
                        onChange={(e) =>
                          setParamValues((prev) => ({
                            ...prev,
                            [p.name]:
                              p.type === "number"
                                ? Number(e.target.value)
                                : e.target.value,
                          }))
                        }
                        placeholder={String(p.default ?? "")}
                        className="w-48 rounded-md border border-zinc-200 bg-white px-2 py-1 text-xs dark:border-zinc-700 dark:bg-zinc-900"
                      />
                    )}
                    <span className="text-[10px] text-zinc-400">({p.type})</span>
                  </div>
                ))}
              </div>
            )}

            <button
              onClick={handleExecute}
              disabled={executing}
              className="mt-3 rounded-md bg-emerald-600 px-4 py-1.5 text-xs font-medium text-white disabled:opacity-50 hover:bg-emerald-700"
            >
              {executing ? "Executing..." : "Execute Report"}
            </button>
          </div>

          {/* Results */}
          {result && (
            <div>
              <label className="text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
                Results ({result.rows.length} rows)
              </label>
              <div className="mt-1 max-h-64 overflow-auto rounded-md border border-zinc-200 dark:border-zinc-700">
                <table className="w-full text-xs">
                  <thead>
                    <tr className="border-b border-zinc-200 bg-zinc-50 dark:border-zinc-700 dark:bg-zinc-800">
                      {result.columns.map((col) => (
                        <th
                          key={col}
                          className="px-3 py-1.5 text-left font-medium text-zinc-600 dark:text-zinc-400"
                        >
                          {col}
                        </th>
                      ))}
                    </tr>
                  </thead>
                  <tbody>
                    {result.rows.slice(0, 50).map((row, i) => (
                      <tr
                        key={i}
                        className="border-b border-zinc-100 dark:border-zinc-800"
                      >
                        {result.columns.map((col) => (
                          <td
                            key={col}
                            className="px-3 py-1 text-zinc-700 dark:text-zinc-300"
                          >
                            {formatCellValue(row[col])}
                          </td>
                        ))}
                      </tr>
                    ))}
                  </tbody>
                </table>
                {result.rows.length > 50 && (
                  <div className="px-3 py-1.5 text-[10px] text-zinc-400">
                    Showing 50 of {result.rows.length} rows
                  </div>
                )}
              </div>
            </div>
          )}
        </>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Report creation form
// ---------------------------------------------------------------------------

function ReportCreateForm({
  ontologyId,
  onSubmit,
}: {
  ontologyId: string;
  onSubmit: (values: ReportCreateRequest) => Promise<void>;
}) {
  const [isOpen, setIsOpen] = useState(false);
  const [isSaving, setIsSaving] = useState(false);

  const [title, setTitle] = useState("");
  const [description, setDescription] = useState("");
  const [queryTemplate, setQueryTemplate] = useState("");
  const [widgetType, setWidgetType] = useState("");
  const [isPublic, setIsPublic] = useState(false);
  const [paramInput, setParamInput] = useState("");

  const reset = () => {
    setTitle("");
    setDescription("");
    setQueryTemplate("");
    setWidgetType("");
    setIsPublic(false);
    setParamInput("");
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!title.trim() || !queryTemplate.trim()) return;
    setIsSaving(true);
    try {
      const parameters = parseParameterInput(paramInput);
      await onSubmit({
        ontology_id: ontologyId,
        title: title.trim(),
        description: description.trim() || undefined,
        query_template: queryTemplate,
        parameters,
        widget_type: widgetType || undefined,
        is_public: isPublic,
      });
      reset();
      setIsOpen(false);
    } catch {
      toast.error("Failed to create report");
    } finally {
      setIsSaving(false);
    }
  };

  if (!isOpen) {
    return (
      <button
        onClick={() => setIsOpen(true)}
        className="mt-4 rounded-md bg-emerald-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-emerald-700"
      >
        New Report
      </button>
    );
  }

  return (
    <form
      onSubmit={handleSubmit}
      className="mt-4 rounded-lg border border-emerald-200 bg-emerald-50/50 p-4 dark:border-emerald-800 dark:bg-emerald-950/20"
    >
      <div className="mb-3 flex items-center justify-between">
        <span className="text-xs font-semibold text-emerald-700 dark:text-emerald-400">
          New Report
        </span>
        <button
          type="button"
          onClick={() => {
            reset();
            setIsOpen(false);
          }}
          className="text-xs text-zinc-400 hover:text-zinc-600"
        >
          Cancel
        </button>
      </div>

      <div className="space-y-3">
        <div>
          <label className="text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
            Title
          </label>
          <input
            value={title}
            onChange={(e) => setTitle(e.target.value)}
            placeholder="Report title"
            required
            className="mt-0.5 w-full rounded-md border border-zinc-200 bg-white px-3 py-1.5 text-xs dark:border-zinc-700 dark:bg-zinc-900"
          />
        </div>

        <div>
          <label className="text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
            Description
          </label>
          <textarea
            value={description}
            onChange={(e) => setDescription(e.target.value)}
            placeholder="What this report shows..."
            rows={2}
            className="mt-0.5 w-full rounded-md border border-zinc-200 bg-white px-3 py-1.5 text-xs dark:border-zinc-700 dark:bg-zinc-900"
          />
        </div>

        <div>
          <label className="text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
            Query Template
          </label>
          <textarea
            value={queryTemplate}
            onChange={(e) => setQueryTemplate(e.target.value)}
            placeholder={"MATCH (n:Product) WHERE n.category = $category RETURN n.name, n.price LIMIT $limit"}
            rows={6}
            required
            className="mt-0.5 w-full rounded-md border border-zinc-200 bg-white px-3 py-1.5 font-mono text-xs dark:border-zinc-700 dark:bg-zinc-900"
          />
        </div>

        <div>
          <label className="text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
            Parameters (JSON)
          </label>
          <textarea
            value={paramInput}
            onChange={(e) => setParamInput(e.target.value)}
            placeholder={`[{"name":"category","type":"string","default":"all","label":"Category"}]`}
            rows={3}
            className="mt-0.5 w-full rounded-md border border-zinc-200 bg-white px-3 py-1.5 font-mono text-xs dark:border-zinc-700 dark:bg-zinc-900"
          />
          <p className="mt-0.5 text-[10px] text-zinc-400">
            Array of {"{"}&quot;name&quot;, &quot;type&quot;: string|number|boolean, &quot;default&quot;, &quot;label&quot;{"}"}
          </p>
        </div>

        <div>
          <label className="text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
            Widget Type
          </label>
          <SettingsSelect
            value={widgetType}
            onChange={(e) => setWidgetType(e.target.value)}
          >
            <option value="">Auto</option>
            {WIDGET_TYPES.map((t) => (
              <option key={t.value} value={t.value}>
                {t.label}
              </option>
            ))}
          </SettingsSelect>
        </div>

        <div className="flex items-center gap-2">
          <label className="text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
            Public
          </label>
          <button
            type="button"
            onClick={() => setIsPublic(!isPublic)}
            className={`relative inline-flex h-5 w-9 items-center rounded-full transition-colors ${
              isPublic ? "bg-emerald-500" : "bg-zinc-300 dark:bg-zinc-600"
            }`}
          >
            <span
              className={`inline-block h-3.5 w-3.5 transform rounded-full bg-white transition-transform ${
                isPublic ? "translate-x-4.5" : "translate-x-0.5"
              }`}
            />
          </button>
        </div>

        <button
          type="submit"
          disabled={!title.trim() || !queryTemplate.trim() || isSaving}
          className="rounded-md bg-emerald-600 px-4 py-1.5 text-xs font-medium text-white disabled:opacity-50 hover:bg-emerald-700"
        >
          {isSaving ? "Creating..." : "Create Report"}
        </button>
      </div>
    </form>
  );
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function parseParameterInput(input: string): ReportParameter[] {
  if (!input.trim()) return [];
  try {
    const parsed = JSON.parse(input);
    if (!Array.isArray(parsed)) return [];
    return parsed;
  } catch {
    return [];
  }
}

function formatCellValue(value: unknown): string {
  if (value == null) return "";
  if (typeof value === "object") return JSON.stringify(value);
  return String(value);
}
