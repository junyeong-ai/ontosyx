"use client";

import { useCallback, useEffect, useState } from "react";
import { useTranslations } from "next-intl";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import { Spinner } from "@/components/ui/spinner";
import { SettingsInput, SettingsSelect, SettingsSwitch } from "@/components/ui/form-input";
import { useConfirm } from "@/components/ui/confirm-dialog";
import { useAuth } from "@/lib/use-auth";
import {
  listFederationAdapters,
  registerFederationAdapter,
  deleteFederationAdapter,
  refreshFederationAdapters,
  getFederationHealth,
  previewFederationAdapter,
  type Credential,
  type FederationAdapterSummary,
  type FederationHealthResponse,
  type RegisterFederationAdapterRequest,
  type PreviewFederationAdapterRequest,
  type PreviewFederationAdapterResponse,
} from "@/lib/api";

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
  const t = useTranslations("settings.federation");
  const { isAdmin } = useAuth();
  const confirm = useConfirm();

  const [adapters, setAdapters] = useState<FederationAdapterSummary[]>([]);
  const [health, setHealth] = useState<FederationHealthResponse | null>(null);
  const [loading, setLoading] = useState(true);
  const [submitting, setSubmitting] = useState(false);
  const [form, setForm] = useState<FormState>(INITIAL_FORM);
  // Preview state — separate from `submitting` so the user can refine
  // the form between a preview and a final register without the
  // preview panel disappearing. `null` = not yet previewed; a stored
  // response stays visible until the user edits + previews again.
  const [previewing, setPreviewing] = useState(false);
  const [preview, setPreview] = useState<PreviewFederationAdapterResponse | null>(null);

  const reload = useCallback(async () => {
    try {
      const [list, h] = await Promise.all([
        listFederationAdapters(),
        getFederationHealth(),
      ]);
      setAdapters(list);
      setHealth(h);
    } catch {
      toast.error(t("toast.loadFailed"));
    } finally {
      setLoading(false);
    }
  }, [t]);

  useEffect(() => {
    if (!isAdmin) {
      setLoading(false);
      return;
    }
    reload();
  }, [isAdmin, reload]);

  const handleSubmit = useCallback(
    async (e: React.FormEvent) => {
      e.preventDefault();
      if (!form.sourceId.trim()) return;
      setSubmitting(true);
      try {
        const req = formToRequest(form);
        await registerFederationAdapter(req);
        toast.success(t("toast.registered", { sourceId: form.sourceId }));
        setForm(INITIAL_FORM);
        await reload();
      } catch {
        toast.error(t("toast.registerFailed"));
      } finally {
        setSubmitting(false);
      }
    },
    [form, reload, t],
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
      try {
        await deleteFederationAdapter(sourceId);
        toast.success(t("toast.deleted", { sourceId }));
        await reload();
      } catch {
        toast.error(t("toast.deleteFailed"));
      }
    },
    [confirm, reload, t],
  );

  const handleRefresh = useCallback(async () => {
    try {
      const { count } = await refreshFederationAdapters();
      toast.success(t("toast.refreshed", { count }));
      await reload();
    } catch {
      toast.error(t("toast.refreshFailed"));
    }
  }, [reload, t]);

  /// Preview dry-runs the adapter against the live source: connects,
  /// lists tables, describes each. No persistence — nothing enters the
  /// store or the resolver. Errors surface as a toast but keep the
  /// form populated so the user can correct + retry.
  const handlePreview = useCallback(async () => {
    setPreviewing(true);
    try {
      const req = formToPreviewRequest(form);
      const resp = await previewFederationAdapter(req);
      setPreview(resp);
    } catch {
      toast.error(t("toast.previewFailed"));
    } finally {
      setPreviewing(false);
    }
  }, [form, t]);

  if (!isAdmin) {
    return (
      <div className="p-6">
        <h1 className="text-2xl font-semibold">{t("title")}</h1>
        <p className="mt-2 text-sm text-neutral-600 dark:text-neutral-400">
          {t("adminOnly")}
        </p>
      </div>
    );
  }

  if (loading) {
    return (
      <div className="flex items-center justify-center p-12">
        <Spinner />
      </div>
    );
  }

  return (
    <div className="space-y-8 p-6">
      <header>
        <h1 className="text-2xl font-semibold">{t("title")}</h1>
        <p className="mt-2 text-sm text-neutral-600 dark:text-neutral-400">
          {t("description")}
        </p>
      </header>

      {health && (
        <section className="rounded-lg border border-neutral-200 bg-neutral-50 p-4 text-sm dark:border-neutral-800 dark:bg-neutral-900">
          <div className="flex items-center justify-between">
            <h2 className="font-medium">{t("health.title")}</h2>
            <Button size="sm" variant="outline" onClick={handleRefresh}>
              {t("health.refresh")}
            </Button>
          </div>
          <dl className="mt-3 grid grid-cols-2 gap-2 md:grid-cols-4">
            <div>
              <dt className="text-xs text-neutral-500">{t("health.hydrated")}</dt>
              <dd>{health.resolver_hydrated ? "✓" : "–"}</dd>
            </div>
            <div>
              <dt className="text-xs text-neutral-500">{t("health.resolverCount")}</dt>
              <dd>{health.resolver_count}</dd>
            </div>
            <div>
              <dt className="text-xs text-neutral-500">{t("health.storeCount")}</dt>
              <dd>{health.store_count}</dd>
            </div>
            <div>
              <dt className="text-xs text-neutral-500">
                {health.in_sync ? t("health.inSync") : t("health.outOfSync")}
              </dt>
              <dd>{health.in_sync ? "✓" : "⚠"}</dd>
            </div>
          </dl>
          {(health.orphans_in_resolver.length > 0 ||
            health.missing_from_resolver.length > 0) && (
            <ul className="mt-3 space-y-1 text-xs text-amber-700 dark:text-amber-400">
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
        <h2 className="mb-3 text-lg font-medium">{t("register.title")}</h2>
        <form onSubmit={handleSubmit} className="space-y-4 rounded-lg border border-neutral-200 bg-white p-4 dark:border-neutral-800 dark:bg-neutral-950">
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
          <p className="text-xs text-neutral-500">{t("register.secretRefHint")}</p>
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
          <div className="mt-4 rounded-lg border border-emerald-200 bg-emerald-50/50 p-4 text-sm dark:border-emerald-900/40 dark:bg-emerald-950/20">
            <div className="mb-3 flex items-center justify-between">
              <div>
                <h3 className="text-sm font-medium">
                  {t("preview.title", { sourceType: preview.source_type })}
                </h3>
                <p className="mt-0.5 text-xs text-neutral-500 dark:text-neutral-400">
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
              <p className="text-xs text-neutral-500 dark:text-neutral-400">
                {t("preview.empty")}
              </p>
            ) : (
              <div className="max-h-80 space-y-3 overflow-auto">
                {preview.tables.map((tbl) => (
                  <details
                    key={tbl.name}
                    className="rounded border border-neutral-200 bg-white dark:border-neutral-800 dark:bg-neutral-950"
                    open={preview.tables.length <= 3}
                  >
                    <summary className="cursor-pointer px-3 py-2 text-xs font-mono font-medium">
                      {tbl.name}{" "}
                      <span className="text-neutral-500">
                        ({tbl.columns.length})
                      </span>
                    </summary>
                    <table className="w-full text-xs">
                      <thead>
                        <tr className="border-t border-neutral-100 text-left text-neutral-500 dark:border-neutral-800">
                          <th className="py-1.5 pl-3 pr-4">
                            {t("preview.columnName")}
                          </th>
                          <th className="py-1.5 pr-4">
                            {t("preview.columnType")}
                          </th>
                          <th className="py-1.5 pr-3">
                            {t("preview.columnNullable")}
                          </th>
                        </tr>
                      </thead>
                      <tbody>
                        {tbl.columns.map((col) => (
                          <tr
                            key={col.name}
                            className="border-t border-neutral-100 dark:border-neutral-900"
                          >
                            <td className="py-1 pl-3 pr-4 font-mono">
                              {col.name}
                            </td>
                            <td className="py-1 pr-4">{col.data_type}</td>
                            <td className="py-1 pr-3 text-neutral-500">
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
          <p className="py-8 text-center text-sm text-neutral-500">{t("empty")}</p>
        ) : (
          <table className="w-full text-sm">
            <thead>
              <tr className="border-b border-neutral-200 text-left dark:border-neutral-800">
                <th className="py-3 pr-6">{t("list.sourceId")}</th>
                <th className="py-3 pr-6">{t("list.kind")}</th>
                <th className="py-3 pr-6">{t("list.actions")}</th>
              </tr>
            </thead>
            <tbody>
              {adapters.map((a) => (
                <tr
                  key={a.source_id}
                  className="border-b border-neutral-100 dark:border-neutral-900"
                >
                  <td className="py-3 pr-6 font-mono">{a.source_id}</td>
                  <td className="py-3 pr-6">{a.source_type}</td>
                  <td className="py-3 pr-6">
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
  );
}
