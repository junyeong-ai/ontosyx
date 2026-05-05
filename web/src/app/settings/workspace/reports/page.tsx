"use client";

import { useEffect, useState } from "react";
import {
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";
import { useTranslations } from "next-intl";
import { z } from "zod";
import { toast } from "@/components/ui/toast";
import { Heading } from "@/components/ui/heading";

import { ErrorState } from "@/components/ui/error-state";
import { PageStateView } from "@/components/layout/page-state-view";
import type { PageState } from "@/components/layout/page-state";
import { SkeletonList } from "@/components/ui/skeleton";
import { SettingsPageShell } from "@/components/layout/settings-page-shell";
import { FormInput, FormTextarea, SettingsSelect } from "@/components/ui/form-input";
import { useConfirm } from "@/components/providers/confirm-provider";
import type {
  SavedReport,
  OntologyListItem,
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
  getWorkspaceOntology,
} from "@/lib/api";
import { WIDGET_TYPES } from "@/components/dashboard/widgets/widget-types";
import { useQueryState } from "@/hooks/use-query-state";

// ---------------------------------------------------------------------------
// Known widget type guard — gates dynamic t(`widgetType.<key>`) calls so
// unexpected backend values fall back to the raw string rather than throwing.
// ---------------------------------------------------------------------------

const WIDGET_TYPE_VALUES = WIDGET_TYPES.map((w) => w.value);
type KnownWidgetType = (typeof WIDGET_TYPES)[number]["value"];

function isKnownWidgetType(s: string): s is KnownWidgetType {
  return (WIDGET_TYPE_VALUES as readonly string[]).includes(s);
}

const reportsKeys = {
  all: ["reports"] as const,
  ontologies: () => [...reportsKeys.all, "ontologies"] as const,
  list: (lineageId: string) => [...reportsKeys.all, "list", lineageId] as const,
};

export default function ReportsPage() {
  const t = useTranslations("settings.workspace.reports");
  const tCommon = useTranslations("common");
  const qc = useQueryClient();
  const confirm = useConfirm();

  // URL-backed filter + selection. Sharing a URL with `?ontology=X&report=Y`
  // restores the exact view — useful when pointing teammates at a saved
  // query. Zero debounce on filter changes (select inputs fire one event).
  const [ontologyFilter, setOntologyFilter] = useQueryState("ontology", {
    default: "",
    parser: z.string(),
    debounceMs: 0,
  });
  const [selectedId, setSelectedId] = useQueryState<string | null>("report", {
    default: null,
    parser: z.union([z.string(), z.null()]),
    debounceMs: 0,
  });

  const ontologiesQuery = useQuery({
    queryKey: reportsKeys.ontologies(),
    queryFn: () => getWorkspaceOntology(),
  });
  const ontology = ontologiesQuery.data ?? null;
  const ontologies = ontology ? [ontology] : [];

  // Reports index by `ontology_lineage_id` (the cross-version handle).
  // With workspace × ontology = 1:1 there's only one lineage to filter
  // by — pin it as soon as we have it.
  const firstLineageId = ontology?.lineage_id;
  useEffect(() => {
    if (firstLineageId && !ontologyFilter) {
      setOntologyFilter(firstLineageId);
    }
  }, [firstLineageId, ontologyFilter, setOntologyFilter]);

  const reportsQuery = useQuery({
    queryKey: reportsKeys.list(ontologyFilter),
    queryFn: () =>
      listReports({ ontology_lineage_id: ontologyFilter }).then((p) => p.items),
    enabled: !!ontologyFilter,
  });
  const reports: SavedReport[] = reportsQuery.data ?? [];

  const deleteMutation = useMutation({
    mutationFn: (id: string) => deleteReport(id),
    onSuccess: (_data, id) => {
      qc.invalidateQueries({ queryKey: reportsKeys.list(ontologyFilter) });
      if (selectedId === id) setSelectedId(null);
      toast.success(t("toast.deleted"));
    },
    onError: () => toast.error(t("toast.deleteFailed")),
  });

  const createMutation = useMutation({
    mutationFn: (values: ReportCreateRequest) => createReport(values),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: reportsKeys.list(ontologyFilter) });
      toast.success(t("toast.created"));
    },
  });

  const updateMutation = useMutation({
    mutationFn: ({ id, patch }: { id: string; patch: ReportUpdateRequest }) =>
      updateReport(id, patch),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: reportsKeys.list(ontologyFilter) });
      toast.success(t("toast.updated"));
    },
    onError: () => toast.error(t("toast.updateFailed")),
  });

  const handleDelete = async (id: string) => {
    const report = reports.find((r) => r.id === id);
    const ok = await confirm({
      title: t("deleteConfirm.title", { name: report?.title ?? id }),
      description: t("deleteConfirm.description"),
      variant: "danger",
    });
    if (!ok) return;
    deleteMutation.mutate(id);
  };

  const handleCreate = async (values: ReportCreateRequest): Promise<void> => {
    await createMutation.mutateAsync(values);
  };

  const handleUpdate = (id: string, patch: ReportUpdateRequest) =>
    updateMutation.mutate({ id, patch });

  if (ontologiesQuery.isLoading || ontologiesQuery.isError) {
    const pageState: PageState = ontologiesQuery.isLoading
      ? { kind: "loading" }
      : { kind: "error", onRetry: () => void ontologiesQuery.refetch() };
    return (
      <SettingsPageShell title={t("title")} subtitle={t("description")}>
        <PageStateView
          state={pageState}
          skeleton={<SkeletonList count={4} />}
          error={{
            title: tCommon("loadError.title"),
            description: tCommon("loadError.description"),
            retryLabel: tCommon("retry"),
          }}
        >
          <></>
        </PageStateView>
      </SettingsPageShell>
    );
  }

  const selected = reports.find((r) => r.id === selectedId);
  const reportsLoading = reportsQuery.isLoading && !!ontologyFilter;

  return (
    <SettingsPageShell title={t("title")} subtitle={t("description")}>
      {/* Ontology filter */}
      <div className="mt-4">
        <label className="text-2xs font-semibold uppercase tracking-wider text-foreground-muted">
          {t("ontologyLabel")}
        </label>
        <SettingsSelect
            label={t("ontologyFilterLabel")}
            hideLabel
          value={ontologyFilter}
          onChange={(e) => {
            setOntologyFilter(e.target.value);
            setSelectedId(null);
          }}
          className="w-64"
        >
          {ontologies.map((o) => (
            <option key={o.id} value={o.lineage_id}>
              {t("ontologyOption", {
                name: o.name,
                version: o.current_version?.version ?? "—",
              })}
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

      {reportsLoading ? (
        <div className="mt-6">
          <SkeletonList count={3} />
        </div>
      ) : reportsQuery.isError && ontologyFilter ? (
        <div className="mt-6">
          <ErrorState
            title={tCommon("loadError.title")}
            description={tCommon("loadError.description")}
            onRetry={() => reportsQuery.refetch()}
            retryLabel={tCommon("retry")}
          />
        </div>
      ) : (
        <div className="mt-6 flex gap-6">
          {/* Report list */}
          <div className="w-72 shrink-0 space-y-1">
            {reports.length === 0 ? (
              <p className="text-sm text-foreground-muted">
                {t("emptyForOntology")}
              </p>
            ) : (
              reports.map((r) => (
                <button
                  type="button"
                  key={r.id}
                  onClick={() => setSelectedId(r.id)}
                  className={`w-full rounded-md px-3 py-2 text-start text-sm transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)] ${
                    r.id === selectedId
                      ? "bg-brand-surface text-brand-foreground-strong"
                      : "text-foreground hover:bg-surface-raised-muted"
                  }`}
                >
                  <div className="flex items-center gap-2">
                    <span className="font-medium truncate">{r.title}</span>
                    <span
                      className={`shrink-0 rounded-full px-1.5 py-0.5 text-2xs font-medium ${
                        r.is_public
                          ? "bg-success-surface text-success-foreground"
                          : "bg-surface-inset text-foreground-muted"
                      }`}
                    >
                      {r.is_public ? t("visibility.public") : t("visibility.private")}
                    </span>
                  </div>
                  <div className="text-xs text-foreground-muted">
                    {r.widget_type
                      ? (isKnownWidgetType(r.widget_type)
                          ? t(`widgetType.${r.widget_type}`)
                          : r.widget_type)
                      : t("widgetAuto")}
                    {" · "}
                    {t("paramsCount", { count: r.parameters.length })}
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
              <div className="text-sm text-foreground-muted">
                {t("selectPrompt")}
              </div>
            )}
          </div>
        </div>
      )}
    </SettingsPageShell>
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
  const t = useTranslations("settings.workspace.reports");
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
  }, [report.title, report.description, report.query_template, report.widget_type, report.is_public, report.parameters]);

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
      toast.error(t("toast.executeError"));
    } finally {
      setExecuting(false);
    }
  };

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <div>
          <Heading level={2} size={6}>
            {report.title}
          </Heading>
          <p className="text-xs text-foreground-muted">
            {t("detail.meta", {
              created: new Date(report.created_at).toLocaleDateString(),
              updated: new Date(report.updated_at).toLocaleDateString(),
            })}
          </p>
        </div>
        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={() => setEditing(!editing)}
            className="rounded-md px-3 py-1.5 text-xs font-medium text-foreground hover:bg-surface-raised"
          >
            {editing ? t("detail.cancel") : t("detail.edit")}
          </button>
          <button
            type="button"
            onClick={async () => {
              const ok = await confirm({
                title: t("deleteConfirm.title", { name: report.title }),
                description: t("deleteConfirm.description"),
                variant: "danger",
              });
              if (ok) onDelete(report.id);
            }}
            className="rounded-md px-3 py-1.5 text-xs font-medium text-danger-foreground hover:bg-danger-surface"
          >
            {t("detail.delete")}
          </button>
        </div>
      </div>

      {/* Inline edit form */}
      {editing ? (
        <div className="space-y-3 rounded-lg border border-brand-border bg-brand-surface p-4">
          <div>
            <label htmlFor="edit-report-title" className="text-2xs font-semibold uppercase tracking-wider text-foreground-muted">
              {t("edit.title")}
            </label>
            <FormInput
              id="edit-report-title"
              value={editTitle}
              onChange={(e) => setEditTitle(e.target.value)}
              className="mt-0.5 text-xs"
            />
          </div>
          <div>
            <label htmlFor="edit-report-description" className="text-2xs font-semibold uppercase tracking-wider text-foreground-muted">
              {t("edit.description")}
            </label>
            <FormTextarea
              id="edit-report-description"
              value={editDescription}
              onChange={(e) => setEditDescription(e.target.value)}
              rows={2}
              className="mt-0.5 text-xs"
            />
          </div>
          <div>
            <label className="text-2xs font-semibold uppercase tracking-wider text-foreground-muted">
              {t("edit.queryTemplate")}
            </label>
            <FormTextarea
              value={editQueryTemplate}
              onChange={(e) => setEditQueryTemplate(e.target.value)}
              rows={6}
              className="mt-0.5 font-mono text-xs"
            />
          </div>
          <div>
            <label className="text-2xs font-semibold uppercase tracking-wider text-foreground-muted">
              {t("edit.widgetType")}
            </label>
            <SettingsSelect
            label={t("edit.widgetTypeSelectLabel")}
            hideLabel
              value={editWidgetType}
              onChange={(e) => setEditWidgetType(e.target.value)}
            >
              <option value="">{t("widgetTypeAuto")}</option>
              {WIDGET_TYPES.map((w) => (
                <option key={w.value} value={w.value}>
                  {t(`widgetType.${w.value}`)}
                </option>
              ))}
            </SettingsSelect>
          </div>
          <div className="flex items-center gap-2">
            <label className="text-2xs font-semibold uppercase tracking-wider text-foreground-muted">
              {t("edit.public")}
            </label>
            <button
              type="button"
              onClick={() => setEditIsPublic(!editIsPublic)}
              className={`relative inline-flex h-5 w-9 items-center rounded-full transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)] ${
                editIsPublic ? "bg-brand-solid" : "bg-surface-raised"
              }`}
            >
              <span
                className={`inline-block h-3.5 w-3.5 transform rounded-full bg-surface-base transition-transform duration-[var(--duration-quick)] ease-[var(--ease-out)] ${
                  editIsPublic ? "translate-x-4.5" : "translate-x-0.5"
                }`}
              />
            </button>
          </div>
          <button
            type="button"
            onClick={handleSaveEdit}
            className="rounded-md bg-brand-solid px-4 py-1.5 text-xs font-medium text-foreground-onbrand hover:bg-brand-solid"
          >
            {t("edit.save")}
          </button>
        </div>
      ) : (
        <>
          {report.description && (
            <div>
              <label className="text-2xs font-semibold uppercase tracking-wider text-foreground-muted">
                {t("detail.description")}
              </label>
              <p className="mt-0.5 text-sm text-foreground">
                {report.description}
              </p>
            </div>
          )}

          <div>
            <label className="text-2xs font-semibold uppercase tracking-wider text-foreground-muted">
              {t("detail.queryTemplate")}
            </label>
            <pre className="mt-1 max-h-48 overflow-auto rounded-md bg-surface-base p-3 text-xs text-brand-foreground">
              {report.query_template}
            </pre>
          </div>

          {report.widget_type && (
            <div>
              <label className="text-2xs font-semibold uppercase tracking-wider text-foreground-muted">
                {t("detail.widgetType")}
              </label>
              <span className="ms-2 rounded bg-surface-inset px-1.5 py-0.5 text-xs text-foreground">
                {isKnownWidgetType(report.widget_type)
                  ? t(`widgetType.${report.widget_type}`)
                  : report.widget_type}
              </span>
            </div>
          )}

          {/* Parameters + execute */}
          <div>
            <label className="text-2xs font-semibold uppercase tracking-wider text-foreground-muted">
              {t("detail.parameters")}
            </label>
            {report.parameters.length === 0 ? (
              <p className="mt-0.5 text-xs text-foreground-muted">{t("detail.noParameters")}</p>
            ) : (
              <div className="mt-1 space-y-2">
                {report.parameters.map((p) => (
                  <div key={p.name} className="flex items-center gap-2">
                    <span className="w-28 shrink-0 text-xs font-medium text-foreground">
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
                        className={`relative inline-flex h-5 w-9 items-center rounded-full transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)] ${
                          paramValues[p.name]
                            ? "bg-brand-solid"
                            : "bg-surface-raised"
                        }`}
                      >
                        <span
                          className={`inline-block h-3.5 w-3.5 transform rounded-full bg-surface-base transition-transform duration-[var(--duration-quick)] ease-[var(--ease-out)] ${
                            paramValues[p.name]
                              ? "translate-x-4.5"
                              : "translate-x-0.5"
                          }`}
                        />
                      </button>
                    ) : (
                      <FormInput
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
                        className="w-48 rounded-md border border-divider bg-surface-base px-2 py-1 text-xs"
                      />
                    )}
                    <span className="text-2xs text-foreground-muted">({p.type})</span>
                  </div>
                ))}
              </div>
            )}

            <button
              type="button"
              onClick={handleExecute}
              disabled={executing}
              className="mt-3 rounded-md bg-brand-solid px-4 py-1.5 text-xs font-medium text-foreground-onbrand disabled:opacity-50 hover:bg-brand-solid"
            >
              {executing ? t("detail.executing") : t("detail.executeReport")}
            </button>
          </div>

          {/* Results */}
          {result && (
            <div>
              <label className="text-2xs font-semibold uppercase tracking-wider text-foreground-muted">
                {t("detail.results", { count: result.rows.length })}
              </label>
              <div className="mt-1 max-h-64 overflow-auto rounded-md border border-divider">
                <table className="w-full text-xs">
                  <thead>
                    <tr className="border-b border-divider bg-surface-raised">
                      {result.columns.map((col) => (
                        <th
                          key={col}
                          className="px-3 py-1.5 text-start font-medium text-foreground"
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
                        className="border-b border-divider-soft"
                      >
                        {result.columns.map((col) => (
                          <td
                            key={col}
                            className="px-3 py-1 text-foreground"
                          >
                            {formatCellValue(row[col])}
                          </td>
                        ))}
                      </tr>
                    ))}
                  </tbody>
                </table>
                {result.rows.length > 50 && (
                  <div className="px-3 py-1.5 text-2xs text-foreground-muted">
                    {t("detail.showingRows", { count: result.rows.length })}
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
  const t = useTranslations("settings.workspace.reports");
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
        ontology_lineage_id: ontologyId,
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
      toast.error(t("toast.createFailed"));
    } finally {
      setIsSaving(false);
    }
  };

  if (!isOpen) {
    return (
      <button
        type="button"
        onClick={() => setIsOpen(true)}
        className="mt-4 rounded-md bg-brand-solid px-3 py-1.5 text-xs font-medium text-foreground-onbrand hover:bg-brand-solid-hover"
      >
        {t("newReport")}
      </button>
    );
  }

  return (
    <form
      onSubmit={handleSubmit}
      className="mt-4 rounded-lg border border-brand-border bg-brand-surface p-4"
    >
      <div className="mb-3 flex items-center justify-between">
        <span className="text-xs font-semibold text-brand-foreground">
          {t("create.newTitle")}
        </span>
        <button
          type="button"
          onClick={() => {
            reset();
            setIsOpen(false);
          }}
          className="text-xs text-foreground-muted hover:text-foreground"
        >
          {t("create.cancel")}
        </button>
      </div>

      <div className="space-y-3">
        <div>
          <label htmlFor="new-report-title" className="text-2xs font-semibold uppercase tracking-wider text-foreground-muted">
            {t("create.title")}
          </label>
          <FormInput
            id="new-report-title"
            value={title}
            onChange={(e) => setTitle(e.target.value)}
            placeholder={t("create.titlePlaceholder")}
            required
            className="mt-0.5 text-xs"
          />
        </div>

        <div>
          <label htmlFor="new-report-description" className="text-2xs font-semibold uppercase tracking-wider text-foreground-muted">
            {t("create.description")}
          </label>
          <FormTextarea
            id="new-report-description"
            value={description}
            onChange={(e) => setDescription(e.target.value)}
            placeholder={t("create.descriptionPlaceholder")}
            rows={2}
            className="mt-0.5 text-xs"
          />
        </div>

        <div>
          <label className="text-2xs font-semibold uppercase tracking-wider text-foreground-muted">
            {t("create.queryTemplate")}
          </label>
          <FormTextarea
            value={queryTemplate}
            onChange={(e) => setQueryTemplate(e.target.value)}
            placeholder={t("create.queryTemplatePlaceholder")}
            rows={6}
            required
            className="mt-0.5 font-mono text-xs"
          />
        </div>

        <div>
          <label className="text-2xs font-semibold uppercase tracking-wider text-foreground-muted">
            {t("create.parameters")}
          </label>
          <FormTextarea
            value={paramInput}
            onChange={(e) => setParamInput(e.target.value)}
            placeholder={t("create.parametersPlaceholder")}
            rows={3}
            className="mt-0.5 font-mono text-xs"
          />
          <p className="mt-0.5 text-2xs text-foreground-muted">
            {t("create.parametersHint", { shape: t("create.parametersShape") })}
          </p>
        </div>

        <div>
          <label className="text-2xs font-semibold uppercase tracking-wider text-foreground-muted">
            {t("create.widgetType")}
          </label>
          <SettingsSelect
            label={t("create.widgetTypeSelectLabel")}
            hideLabel
            value={widgetType}
            onChange={(e) => setWidgetType(e.target.value)}
          >
            <option value="">{t("widgetTypeAuto")}</option>
            {WIDGET_TYPES.map((w) => (
              <option key={w.value} value={w.value}>
                {t(`widgetType.${w.value}`)}
              </option>
            ))}
          </SettingsSelect>
        </div>

        <div className="flex items-center gap-2">
          <label className="text-2xs font-semibold uppercase tracking-wider text-foreground-muted">
            {t("create.public")}
          </label>
          <button
            type="button"
            onClick={() => setIsPublic(!isPublic)}
            className={`relative inline-flex h-5 w-9 items-center rounded-full transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)] ${
              isPublic ? "bg-brand-solid" : "bg-surface-raised"
            }`}
          >
            <span
              className={`inline-block h-3.5 w-3.5 transform rounded-full bg-surface-base transition-transform duration-[var(--duration-quick)] ease-[var(--ease-out)] ${
                isPublic ? "translate-x-4.5" : "translate-x-0.5"
              }`}
            />
          </button>
        </div>

        <button
          type="submit"
          disabled={!title.trim() || !queryTemplate.trim() || isSaving}
          className="rounded-md bg-brand-solid px-4 py-1.5 text-xs font-medium text-foreground-onbrand disabled:opacity-50 hover:bg-brand-solid"
        >
          {isSaving ? t("create.creating") : t("create.create")}
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
