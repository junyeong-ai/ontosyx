"use client";

// Φ5 / Φ2-F closeout — selective table picker.
//
// Sits between step 2 (source kind + connection) and step 3
// (glossary draft) so the operator can scope the upcoming
// introspection to the subset of tables they actually want to
// model. Default mode is "all" so a user who clicks straight
// through gets the legacy whole-source behaviour.
//
// Backend wiring: this page calls the existing
// `POST /api/admin/federation/adapters/preview` endpoint with the
// step-2 connection string. The endpoint builds a transient adapter,
// lists tables + describes them, and returns the result without
// persisting. The bootstrap user is presumed admin during workspace
// setup; non-admin users see the 403 surfaced as an inline error.

import { useMemo } from "react";
import { useTranslations } from "next-intl";
import { useQuery } from "@tanstack/react-query";

import { Button } from "@/components/ui/button";
import { Spinner } from "@/components/ui/spinner";
import { request } from "@/lib/api/client";

import { useBootstrap } from "../bootstrap-state";
import { StepShell } from "../step-shell";

interface PreviewTable {
  name: string;
  columns: { name: string; data_type: string; nullable: boolean }[];
}

interface PreviewResponse {
  source_type: string;
  tables: PreviewTable[];
}

// Bootstrap kind strings → backend adapter kind tag. `postgresql`
// in the wizard maps to `postgres` in the federation registry; the
// rest pass through.
function backendKind(wizard: string): string | null {
  switch (wizard) {
    case "postgresql":
      return "postgres";
    case "mysql":
      return "mysql";
    case "bigquery":
      return "bigquery";
    case "csv":
      return "csv";
    case "json":
      return "json";
    default:
      return null;
  }
}

// CSV / JSON are inline payloads — `credential.value` holds the
// data. SQL adapters take the connection string. Both paths use the
// `inline` credential variant in this wizard because the user typed
// the value directly into step 2; secret-ref wiring is a separate
// admin flow.
function buildPreviewBody(
  wizardKind: string,
  connection: string,
): Record<string, unknown> | null {
  const kind = backendKind(wizardKind);
  if (!kind) return null;
  if (kind === "csv" || kind === "json") {
    return { kind, credential: { kind: "inline", value: connection } };
  }
  if (kind === "postgres") {
    return {
      kind,
      credential: { kind: "inline", value: connection },
      // schema_name omitted — backend defaults to "public".
    };
  }
  if (kind === "mysql") {
    // MySQL requires schema_name; the wizard hasn't asked for one
    // (the connection string usually carries the database). We
    // pass an empty string and let the backend's URL parsing pick
    // it up, surfacing an error in the inline preview if it can't.
    return {
      kind,
      credential: { kind: "inline", value: connection },
      schema_name: "",
    };
  }
  if (kind === "bigquery") {
    return { kind, credential: { kind: "inline", value: connection } };
  }
  return null;
}

export default function SelectTablesStep() {
  const t = useTranslations("bootstrap.step2b");
  const { state, update } = useBootstrap();

  // Selected set lives in BootstrapState — survives navigation
  // back from step 3 + page refresh.
  const selected = useMemo(
    () => new Set(state.selectedTables),
    [state.selectedTables],
  );

  const previewBody = useMemo(
    () => buildPreviewBody(state.sourceKind, state.sourceConnection),
    [state.sourceKind, state.sourceConnection],
  );

  // TanStack query — keeps the data fetch out of useEffect (and so
  // out of React 19's `react-hooks/set-state-in-effect` rule's
  // sights). Cache key includes the body so a navigation back-and-
  // forth replays from cache.
  const previewQuery = useQuery<PreviewTable[]>({
    queryKey: ["bootstrap-tables-preview", previewBody],
    queryFn: async () => {
      if (!previewBody) return [];
      const res = await request<{ data: PreviewResponse }>(
        "/admin/federation/adapters/preview",
        { method: "POST", body: JSON.stringify(previewBody) },
      );
      return res.data.tables;
    },
    enabled: previewBody !== null,
  });

  const tables = previewQuery.data ?? null;
  const loading = previewQuery.isLoading;
  const error = previewQuery.error?.message ?? null;

  const toggleTable = (name: string) => {
    const next = new Set(selected);
    if (next.has(name)) next.delete(name);
    else next.add(name);
    update({ selectedTables: Array.from(next) });
  };

  const selectAll = () => {
    if (!tables) return;
    update({ selectedTables: tables.map((t) => t.name) });
  };

  const clearSelection = () => update({ selectedTables: [] });

  const setMode = (mode: "all" | "subset") => update({ analyzeMode: mode });

  // Step advances when:
  // - mode is "all" (no selection required), OR
  // - mode is "subset" and at least one table is selected
  const canAdvance =
    state.analyzeMode === "all" || state.selectedTables.length > 0;

  return (
    <StepShell
      stepKey="2b-select-tables"
      nextPath="/bootstrap/3-glossary"
      backPath="/bootstrap/2-source"
      canAdvance={canAdvance}
      title={t("title")}
      subtitle={t("subtitle")}
    >
      {/* Mode toggle — drives whether the table list is required. */}
      <fieldset
        className="grid grid-cols-2 gap-2"
        aria-label={t("modeLabel")}
      >
        {(["all", "subset"] as const).map((m) => (
          <label
            key={m}
            className={`cursor-pointer rounded border px-3 py-3 text-xs ${
              state.analyzeMode === m
                ? "border-violet-500 bg-violet-50 text-violet-700 dark:bg-violet-950/40 dark:text-violet-300"
                : "border-zinc-200 bg-white text-muted-foreground hover:bg-zinc-50 dark:border-zinc-700 dark:bg-zinc-900 dark:hover:bg-zinc-800"
            }`}
          >
            <input
              type="radio"
              name="analyzeMode"
              value={m}
              checked={state.analyzeMode === m}
              onChange={() => setMode(m)}
              className="sr-only"
            />
            <p className="font-medium">{t(`modes.${m}.label`)}</p>
            <p className="mt-0.5 text-[10px] text-muted-foreground">
              {t(`modes.${m}.hint`)}
            </p>
          </label>
        ))}
      </fieldset>

      {/* Selection panel — only visible in subset mode. The list
          itself loads independently so an "all" user still sees
          the table count once preview returns. */}
      {state.analyzeMode === "subset" && (
        <div className="flex flex-col gap-2">
          <div className="flex items-center justify-between gap-2">
            <p className="text-xs font-medium text-zinc-700 dark:text-zinc-300">
              {tables
                ? t("selectionSummary", {
                    selected: state.selectedTables.length,
                    total: tables.length,
                  })
                : t("selectionLoading")}
            </p>
            <div className="flex items-center gap-1.5">
              <Button
                size="xs"
                variant="ghost"
                onClick={selectAll}
                disabled={!tables}
              >
                {t("selectAll")}
              </Button>
              <Button
                size="xs"
                variant="ghost"
                onClick={clearSelection}
                disabled={state.selectedTables.length === 0}
              >
                {t("clearSelection")}
              </Button>
            </div>
          </div>

          {loading && (
            <div className="flex items-center justify-center py-6">
              <Spinner />
            </div>
          )}

          {error && (
            <p className="rounded border border-rose-200 bg-rose-50 p-3 text-xs text-rose-700 dark:border-rose-900 dark:bg-rose-950/30 dark:text-rose-300">
              {t("error", { error })}
            </p>
          )}

          {tables && tables.length === 0 && (
            <p className="rounded border border-amber-200 bg-amber-50 p-3 text-xs text-amber-700 dark:border-amber-900 dark:bg-amber-950/30 dark:text-amber-300">
              {t("emptyTables")}
            </p>
          )}

          {tables && tables.length > 0 && (
            <ul className="max-h-96 overflow-y-auto rounded border border-zinc-200 bg-white dark:border-zinc-700 dark:bg-zinc-900">
              {tables.map((t) => (
                <li
                  key={t.name}
                  className="border-b border-zinc-100 last:border-b-0 dark:border-zinc-800"
                >
                  <label className="flex cursor-pointer items-center gap-3 px-3 py-2 hover:bg-zinc-50 dark:hover:bg-zinc-800/50">
                    <input
                      type="checkbox"
                      checked={selected.has(t.name)}
                      onChange={() => toggleTable(t.name)}
                      className="h-3.5 w-3.5 rounded border-zinc-300 text-emerald-600 focus:ring-emerald-500"
                    />
                    <span className="font-mono text-xs text-zinc-900 dark:text-zinc-100">
                      {t.name}
                    </span>
                    <span className="ml-auto text-[10px] text-zinc-500 dark:text-zinc-500">
                      {t.columns.length} cols
                    </span>
                  </label>
                </li>
              ))}
            </ul>
          )}
        </div>
      )}
    </StepShell>
  );
}
