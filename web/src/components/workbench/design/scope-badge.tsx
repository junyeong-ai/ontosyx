"use client";

import { useState } from "react";
import { useTranslations } from "next-intl";
import { HugeiconsIcon } from "@hugeicons/react";
import { Database01Icon } from "@hugeicons/core-free-icons";
import { toast } from "sonner";

import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover";
import { useAppStore } from "@/lib/store";
import {
  useDeferScopeTables,
  useIncludeScopeTables,
} from "@/hooks/api/use-projects";
import type { DeferredTable } from "@/types/projects";

export function ScopeBadge() {
  const t = useTranslations("workbench.design.scope");
  const project = useAppStore((s) => s.activeProject);
  const scope = project?.analysis_scope;
  if (!project || !scope) return null;

  const included = scope.included ?? [];
  const deferred = scope.deferred ?? [];
  if (included.length === 0 && deferred.length === 0) return null;

  const summary = t("summary", {
    modeled: included.length,
    deferred: deferred.length,
  });

  return (
    <Popover>
      <PopoverTrigger className="inline-flex items-center gap-1 rounded-md border border-emerald-200 bg-white px-2 py-1 text-[10px] text-emerald-700 shadow-sm hover:bg-emerald-50 dark:border-emerald-900 dark:bg-zinc-900 dark:text-emerald-300 dark:hover:bg-zinc-800">
        <HugeiconsIcon icon={Database01Icon} className="h-3 w-3" size="100%" />
        {summary}
      </PopoverTrigger>
      <PopoverContent className="z-50 w-[360px] rounded-lg border border-zinc-200 bg-white p-3 shadow-xl outline-none dark:border-zinc-700 dark:bg-zinc-900">
        <ScopePanel
          projectId={project.id}
          revision={project.revision}
          included={included}
          deferred={deferred}
        />
      </PopoverContent>
    </Popover>
  );
}

function ScopePanel({
  projectId,
  revision,
  included,
  deferred,
}: {
  projectId: string;
  revision: number;
  included: string[];
  deferred: DeferredTable[];
}) {
  const t = useTranslations("workbench.design.scope");
  const include = useIncludeScopeTables(projectId);
  const defer = useDeferScopeTables(projectId);
  const busy = include.isPending || defer.isPending;

  const [deferTarget, setDeferTarget] = useState<string | null>(null);
  const [reasonDraft, setReasonDraft] = useState("");

  const handlePromote = (table: string) => {
    include.mutate(
      { tables: [table], expected_revision: revision },
      {
        onSuccess: () =>
          toast.success(t("toast.promoted", { table })),
        onError: (err) =>
          toast.error(
            err instanceof Error
              ? t("toast.promoteFailed", { error: err.message })
              : t("toast.promoteFailed", { error: String(err) }),
          ),
      },
    );
  };

  const handleDeferConfirm = (table: string) => {
    const reason = reasonDraft.trim();
    if (!reason) {
      toast.error(t("toast.reasonRequired"));
      return;
    }
    defer.mutate(
      { tables: [table], reason, expected_revision: revision },
      {
        onSuccess: () => {
          toast.success(t("toast.deferred", { table }));
          setDeferTarget(null);
          setReasonDraft("");
        },
        onError: (err) =>
          toast.error(
            err instanceof Error
              ? t("toast.deferFailed", { error: err.message })
              : t("toast.deferFailed", { error: String(err) }),
          ),
      },
    );
  };

  return (
    <div className="flex max-h-[420px] flex-col gap-3 overflow-y-auto text-xs">
      {included.length > 0 && (
        <section>
          <h3 className="mb-1 text-[10px] font-semibold uppercase tracking-wider text-emerald-700 dark:text-emerald-300">
            {t("includedLabel")}
            <span className="ml-1 font-mono text-muted-foreground">
              ({included.length})
            </span>
          </h3>
          <ul className="space-y-0.5">
            {included.map((table) => (
              <li
                key={table}
                className="flex items-center gap-2 rounded px-1.5 py-1 hover:bg-zinc-50 dark:hover:bg-zinc-800/50"
              >
                <span className="flex-1 truncate font-mono text-[11px]">
                  {table}
                </span>
                {deferTarget === table ? (
                  <DeferReasonInline
                    reason={reasonDraft}
                    onChange={setReasonDraft}
                    onConfirm={() => handleDeferConfirm(table)}
                    onCancel={() => {
                      setDeferTarget(null);
                      setReasonDraft("");
                    }}
                    busy={busy}
                  />
                ) : (
                  <button
                    type="button"
                    onClick={() => setDeferTarget(table)}
                    disabled={busy}
                    className="rounded px-1.5 py-0.5 text-[10px] font-medium text-amber-700 hover:bg-amber-50 disabled:opacity-50 dark:text-amber-300 dark:hover:bg-amber-950/40"
                  >
                    {t("actions.defer")}
                  </button>
                )}
              </li>
            ))}
          </ul>
        </section>
      )}

      {deferred.length > 0 && (
        <section>
          <h3 className="mb-1 text-[10px] font-semibold uppercase tracking-wider text-amber-700 dark:text-amber-300">
            {t("deferredLabel")}
            <span className="ml-1 font-mono text-muted-foreground">
              ({deferred.length})
            </span>
          </h3>
          <ul className="space-y-0.5">
            {deferred.map((d) => (
              <li
                key={d.table}
                className="flex items-center gap-2 rounded px-1.5 py-1 hover:bg-zinc-50 dark:hover:bg-zinc-800/50"
              >
                <div className="min-w-0 flex-1">
                  <p className="truncate font-mono text-[11px]">{d.table}</p>
                  <p className="truncate text-[10px] italic text-muted-foreground">
                    {d.reason}
                  </p>
                </div>
                <button
                  type="button"
                  onClick={() => handlePromote(d.table)}
                  disabled={busy}
                  className="rounded px-1.5 py-0.5 text-[10px] font-medium text-emerald-700 hover:bg-emerald-50 disabled:opacity-50 dark:text-emerald-300 dark:hover:bg-emerald-950/40"
                >
                  {t("actions.promote")}
                </button>
              </li>
            ))}
          </ul>
        </section>
      )}
    </div>
  );
}

function DeferReasonInline({
  reason,
  onChange,
  onConfirm,
  onCancel,
  busy,
}: {
  reason: string;
  onChange: (next: string) => void;
  onConfirm: () => void;
  onCancel: () => void;
  busy: boolean;
}) {
  const t = useTranslations("workbench.design.scope");
  return (
    <div className="flex items-center gap-1">
      <input
        autoFocus
        value={reason}
        onChange={(e) => onChange(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter") onConfirm();
          else if (e.key === "Escape") onCancel();
        }}
        placeholder={t("deferReasonPlaceholder")}
        className="w-32 rounded border border-amber-300 bg-white px-1.5 py-0.5 text-[10px] focus:border-amber-500 focus:outline-none dark:border-amber-800 dark:bg-zinc-950"
      />
      <button
        type="button"
        onClick={onConfirm}
        disabled={busy || reason.trim() === ""}
        className="rounded bg-amber-600 px-1.5 py-0.5 text-[10px] font-medium text-white hover:bg-amber-700 disabled:opacity-50"
      >
        {t("actions.deferConfirm")}
      </button>
      <button
        type="button"
        onClick={onCancel}
        disabled={busy}
        className="rounded px-1 py-0.5 text-[10px] text-muted-foreground hover:bg-zinc-100 dark:hover:bg-zinc-800"
        aria-label={t("actions.cancel")}
      >
        ✕
      </button>
    </div>
  );
}
