"use client";

import { useCallback, useState } from "react";
import {
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";
import { useTranslations } from "next-intl";
import { toast } from "@/components/ui/toast";
import { Database } from "lucide-react";
import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/empty-state";
import { Heading } from "@/components/ui/heading";
import { SkeletonCard, SkeletonList } from "@/components/ui/skeleton";
import { SettingsPageShell } from "@/components/layout/settings-page-shell";
import { PageStateView } from "@/components/layout/page-state-view";
import type { PageState } from "@/components/layout/page-state";
import { SettingsInput, SettingsSelect, SettingsSwitch } from "@/components/ui/form-input";
import { useConfirm } from "@/components/providers/confirm-provider";
import { useAuth } from "@/hooks/use-auth";
import {
  listFederationAdapters,
  registerFederationAdapter,
  deleteFederationAdapter,
  refreshFederationAdapters,
  getFederationHealth,
  previewFederationAdapter,
  type Credential,
  type RegisterFederationAdapterRequest,
  type PreviewFederationAdapterRequest,
  type PreviewFederationAdapterResponse,
} from "@/lib/api";

const federationKeys = {
  all: ["federation"] as const,
  adapters: () => [...federationKeys.all, "adapters"] as const,
  health: () => [...federationKeys.all, "health"] as const,
};

type AdapterKind = "csv" | "json" | "postgres" | "mysql" | "bigquery";

type FormState = {
  sourceId: string;
  kind: AdapterKind;
  payload: string;
  connectionString: string;
  schemaName: string;
  useSecretRef: boolean;
};

const INITIAL_FORM: FormState = {
  sourceId: "",
  kind: "csv",
  payload: "",
  connectionString: "",
  schemaName: "",
  useSecretRef: false,
};

function requiresPayload(kind: AdapterKind): boolean {
  return kind === "csv" || kind === "json";
}

function requiresConnectionString(kind: AdapterKind): boolean {
  return kind === "postgres" || kind === "mysql" || kind === "bigquery";
}

/// Build the adapter-kind payload (credential + kind-specific fields)
/// used by both `register` and `preview`. Factored out so both code
/// paths stay aligned — a new kind-specific field added here flows
/// to both surfaces automatically.
function formToPreviewRequest(form: FormState): PreviewFederationAdapterRequest {
  const { kind, payload, connectionString, schemaName, useSecretRef } = form;
  // `Credential` is the one wire shape for both CSV/JSON payloads
  // and DB connection strings — the outer kind chooses which field
  // the server stores it under.
  const makeCredential = (value: string): Credential =>
    useSecretRef ? { kind: "secret_ref", value } : { kind: "inline", value };
  if (kind === "csv" || kind === "json") {
    return { kind, credential: makeCredential(payload) };
  }
  const credential = makeCredential(connectionString);
  if (kind === "postgres") {
    return {
      kind,
      credential,
      ...(schemaName ? { schema_name: schemaName } : {}),
    };
  }
  if (kind === "mysql") {
    // `schema_name` is required at the type level for mysql — the
    // adapter has no default-database equivalent of Postgres' `public`.
    return { kind, credential, schema_name: schemaName };
  }
  // bigquery — no schema_name (dataset lives in the connection URI).
  return { kind: "bigquery", credential };
}

function formToRequest(form: FormState): RegisterFederationAdapterRequest {
  return { source_id: form.sourceId, ...formToPreviewRequest(form) };
}

export default function FederationAdaptersPage() {
  const t = useTranslations("settings.knowledge.federation");
  const tCommon = useTranslations("common");
  const { isAdmin } = useAuth();
  const confirm = useConfirm();
  const qc = useQueryClient();

  const [form, setForm] = useState<FormState>(INITIAL_FORM);
  // Preview state — separate from submission so the user can refine
  // the form between a preview and a final register without the
  // preview panel disappearing.
  const [preview, setPreview] = useState<PreviewFederationAdapterResponse | null>(null);

  const adaptersQuery = useQuery({
    queryKey: federationKeys.adapters(),
    queryFn: () => listFederationAdapters(),
    enabled: isAdmin,
  });
  const healthQuery = useQuery({
    queryKey: federationKeys.health(),
    queryFn: () => getFederationHealth(),
    enabled: isAdmin,
  });

  const adapters = adaptersQuery.data ?? [];
  const health = healthQuery.data;
  const reload = () => {
    qc.invalidateQueries({ queryKey: federationKeys.adapters() });
    qc.invalidateQueries({ queryKey: federationKeys.health() });
  };

  const registerMutation = useMutation({
    mutationFn: (req: RegisterFederationAdapterRequest) =>
      registerFederationAdapter(req),
    onSuccess: () => {
      toast.success(t("toast.registered", { sourceId: form.sourceId }));
      setForm(INITIAL_FORM);
      reload();
    },
    onError: () => toast.error(t("toast.registerFailed")),
  });

  const deleteMutation = useMutation({
    mutationFn: (sourceId: string) => deleteFederationAdapter(sourceId),
    onSuccess: (_data, sourceId) => {
      toast.success(t("toast.deleted", { sourceId }));
      reload();
    },
    onError: () => toast.error(t("toast.deleteFailed")),
  });

  const refreshMutation = useMutation({
    mutationFn: () => refreshFederationAdapters(),
    onSuccess: ({ count }) => {
      toast.success(t("toast.refreshed", { count }));
      reload();
    },
    onError: () => toast.error(t("toast.refreshFailed")),
  });

  const previewMutation = useMutation({
    mutationFn: (req: PreviewFederationAdapterRequest) =>
      previewFederationAdapter(req),
    onSuccess: setPreview,
    onError: () => toast.error(t("toast.previewFailed")),
  });

  const handleSubmit = useCallback(
    (e: React.FormEvent) => {
      e.preventDefault();
      if (!form.sourceId.trim()) return;
      registerMutation.mutate(formToRequest(form));
    },
    [form, registerMutation],
  );

  const handleDelete = useCallback(
    async (sourceId: string) => {
      const ok = await confirm({
        title: t("deleteConfirmTitle", { sourceId }),
        description: t("deleteConfirmDescription"),
        confirmLabel: t("deleteConfirmLabel"),
        variant: "danger",
      });
      if (!ok) return;
      deleteMutation.mutate(sourceId);
    },
    [confirm, deleteMutation, t],
  );

  const handleRefresh = useCallback(() => refreshMutation.mutate(), [refreshMutation]);

  /// Preview dry-runs the adapter against the live source: connects,
  /// lists tables, describes each. No persistence — nothing enters the
  /// store or the resolver. Errors surface as a toast but keep the
  /// form populated so the user can correct + retry.
  const handlePreview = useCallback(() => {
    previewMutation.mutate(formToPreviewRequest(form));
  }, [form, previewMutation]);

  const submitting = registerMutation.isPending;
  const previewing = previewMutation.isPending;

  if (!isAdmin) {
    return (
      <SettingsPageShell title={t("title")} subtitle={t("description")}>
        <EmptyState title={t("adminOnly")} />
      </SettingsPageShell>
    );
  }

  const pageState: PageState =
    adaptersQuery.isLoading || healthQuery.isLoading
      ? { kind: "loading" }
      : adaptersQuery.isError || healthQuery.isError
        ? {
            kind: "error",
            onRetry: () => {
              void adaptersQuery.refetch();
              void healthQuery.refetch();
            },
          }
        : { kind: "data" };

  return (
    <SettingsPageShell title={t("title")} subtitle={t("description")}>
      <PageStateView
        state={pageState}
        skeleton={
          <div className="space-y-4">
            <SkeletonCard />
            <SkeletonList count={3} />
          </div>
        }
        error={{
          title: tCommon("loadError.title"),
          description: tCommon("loadError.description"),
          retryLabel: tCommon("retry"),
        }}
      >
      <div className="space-y-8">
      {health && (
        <section className="rounded-lg border border-divider-soft bg-surface-base p-4 text-sm">
          <div className="flex items-center justify-between">
            <Heading level={2} size={6} className="font-medium">
              {t("health.title")}
            </Heading>
            <Button size="sm" variant="outline" onClick={handleRefresh}>
              {t("health.refresh")}
            </Button>
          </div>
          <dl className="mt-3 grid grid-cols-2 gap-2 md:grid-cols-4">
            <div>
              <dt className="text-xs text-foreground-muted">{t("health.hydrated")}</dt>
              <dd>{health.resolver_hydrated ? "✓" : "–"}</dd>
            </div>
            <div>
              <dt className="text-xs text-foreground-muted">{t("health.resolverCount")}</dt>
              <dd>{health.resolver_count}</dd>
            </div>
            <div>
              <dt className="text-xs text-foreground-muted">{t("health.storeCount")}</dt>
              <dd>{health.store_count}</dd>
            </div>
            <div>
              <dt className="text-xs text-foreground-muted">
                {health.in_sync ? t("health.inSync") : t("health.outOfSync")}
              </dt>
              <dd>{health.in_sync ? "✓" : "⚠"}</dd>
            </div>
          </dl>
          {(health.orphans_in_resolver.length > 0 ||
            health.missing_from_resolver.length > 0) && (
            <ul className="mt-3 space-y-1 text-xs text-warning-foreground">
              {health.orphans_in_resolver.length > 0 && (
                <li>
                  {t("health.orphans", { count: health.orphans_in_resolver.length })}:{" "}
                  {health.orphans_in_resolver.join(", ")}
                </li>
              )}
              {health.missing_from_resolver.length > 0 && (
                <li>
                  {t("health.missing", { count: health.missing_from_resolver.length })}:{" "}
                  {health.missing_from_resolver.join(", ")}
                </li>
              )}
            </ul>
          )}
        </section>
      )}

      <section>
        <Heading level={2} size={4} className="mb-3 font-medium">
          {t("register.title")}
        </Heading>
        <form onSubmit={handleSubmit} className="space-y-4 rounded-lg border border-divider-soft bg-surface-base p-4">
          <SettingsInput
            label={t("register.sourceId")}
            placeholder={t("register.sourceIdPlaceholder")}
            value={form.sourceId}
            onChange={(e) =>
              setForm((f) => ({ ...f, sourceId: e.target.value }))
            }
          />
          <SettingsSelect
            label={t("register.kind")}
            value={form.kind}
            onChange={(e) =>
              setForm((f) => ({ ...f, kind: e.target.value as AdapterKind }))
            }
          >
            <option value="csv">{t("register.csv")}</option>
            <option value="json">{t("register.json")}</option>
            <option value="postgres">{t("register.postgres")}</option>
            <option value="mysql">{t("register.mysql")}</option>
            <option value="bigquery">{t("register.bigquery")}</option>
          </SettingsSelect>
          <SettingsSwitch
            label={t("register.useSecretRef")}
            checked={form.useSecretRef}
            onChange={(v) => setForm((f) => ({ ...f, useSecretRef: v }))}
          />
          <p className="text-xs text-foreground-muted">{t("register.secretRefHint")}</p>
          {requiresPayload(form.kind) && (
            <SettingsInput
              label={t("register.data")}
              placeholder={t("register.dataPlaceholder")}
              value={form.payload}
              onChange={(e) =>
                setForm((f) => ({ ...f, payload: e.target.value }))
              }
            />
          )}
          {requiresConnectionString(form.kind) && (
            <>
              <SettingsInput
                label={t("register.connectionString")}
                placeholder={t("register.connectionStringPlaceholder")}
                value={form.connectionString}
                onChange={(e) =>
                  setForm((f) => ({ ...f, connectionString: e.target.value }))
                }
              />
              {(form.kind === "postgres" || form.kind === "mysql") && (
                <SettingsInput
                  label={t("register.schemaName")}
                  placeholder={t("register.schemaNamePlaceholder")}
                  value={form.schemaName}
                  onChange={(e) =>
                    setForm((f) => ({ ...f, schemaName: e.target.value }))
                  }
                />
              )}
            </>
          )}
          <div className="flex items-center justify-end gap-2">
            <Button
              type="button"
              variant="outline"
              disabled={previewing || submitting}
              onClick={handlePreview}
            >
              {previewing ? t("register.previewing") : t("register.preview")}
            </Button>
            <Button type="submit" disabled={submitting || !form.sourceId.trim()}>
              {submitting ? t("register.submitting") : t("register.submit")}
            </Button>
          </div>
        </form>

        {preview && (
          <div className="mt-4 rounded-lg border border-brand-border bg-brand-surface p-4 text-sm">
            <div className="mb-3 flex items-center justify-between">
              <div>
                <Heading level={3} size={6} className="font-medium">
                  {t("preview.title", { sourceType: preview.source_type })}
                </Heading>
                <p className="mt-0.5 text-xs text-foreground-muted">
                  {t("preview.summary", {
                    tables: preview.tables.length,
                    columns: preview.tables.reduce(
                      (n, tbl) => n + tbl.columns.length,
                      0,
                    ),
                  })}
                </p>
              </div>
              <Button
                size="sm"
                variant="ghost"
                onClick={() => setPreview(null)}
              >
                {t("preview.dismiss")}
              </Button>
            </div>
            {preview.tables.length === 0 ? (
              <p className="text-xs text-foreground-muted">
                {t("preview.empty")}
              </p>
            ) : (
              <div className="max-h-80 space-y-3 overflow-auto">
                {preview.tables.map((tbl) => (
                  <details
                    key={tbl.name}
                    className="rounded border border-divider-soft bg-surface-base"
                    open={preview.tables.length <= 3}
                  >
                    <summary className="cursor-pointer px-3 py-2 text-xs font-mono font-medium">
                      {tbl.name}{" "}
                      <span className="text-foreground-muted">
                        ({tbl.columns.length})
                      </span>
                    </summary>
                    <table className="w-full text-xs">
                      <thead>
                        <tr className="border-t border-divider-soft text-start text-foreground-muted">
                          <th className="py-1.5 ps-3 pe-4">
                            {t("preview.columnName")}
                          </th>
                          <th className="py-1.5 pe-4">
                            {t("preview.columnType")}
                          </th>
                          <th className="py-1.5 pe-3">
                            {t("preview.columnNullable")}
                          </th>
                        </tr>
                      </thead>
                      <tbody>
                        {tbl.columns.map((col) => (
                          <tr
                            key={col.name}
                            className="border-t border-divider-soft"
                          >
                            <td className="py-1 ps-3 pe-4 font-mono">
                              {col.name}
                            </td>
                            <td className="py-1 pe-4">{col.data_type}</td>
                            <td className="py-1 pe-3 text-foreground-muted">
                              {col.nullable ? "✓" : "–"}
                            </td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </details>
                ))}
              </div>
            )}
          </div>
        )}
      </section>

      <section>
        {adapters.length === 0 ? (
          <EmptyState icon={Database} title={t("empty")} />
        ) : (
          <table className="w-full text-sm">
            <thead>
              <tr className="border-b border-divider-soft text-start">
                <th className="py-3 pe-6">{t("list.sourceId")}</th>
                <th className="py-3 pe-6">{t("list.kind")}</th>
                <th className="py-3 pe-6">{t("list.actions")}</th>
              </tr>
            </thead>
            <tbody>
              {adapters.map((a) => (
                <tr
                  key={a.source_id}
                  className="border-b border-divider-soft"
                >
                  <td className="py-3 pe-6 font-mono">{a.source_id}</td>
                  <td className="py-3 pe-6">{a.source_type}</td>
                  <td className="py-3 pe-6">
                    <Button
                      size="sm"
                      variant="ghost"
                      onClick={() => handleDelete(a.source_id)}
                    >
                      {t("deleteConfirmLabel")}
                    </Button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </section>
      </div>
      </PageStateView>
    </SettingsPageShell>
  );
}
