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
      <PopoverTrigger className="inline-flex items-center gap-1 rounded-md border border-brand-border bg-surface-base px-2 py-1 text-2xs text-brand-foreground shadow-sm hover:bg-brand-surface-strong">
        <HugeiconsIcon icon={Database01Icon} className="h-3 w-3" size="100%" />
        {summary}
      </PopoverTrigger>
      <PopoverContent className="z-50 w-inspector rounded-lg border border-divider bg-surface-base p-3 shadow-xl outline-none">
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
          <h3 className="mb-1 text-2xs font-semibold uppercase tracking-wider text-brand-foreground-strong">
            {t("includedLabel")}
            <span className="ml-1 font-mono text-muted-foreground">
              ({included.length})
            </span>
          </h3>
          <ul className="space-y-0.5">
            {included.map((table) => (
              <li
                key={table}
                className="flex items-center gap-2 rounded px-1.5 py-1 hover:bg-surface-raised"
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
                    className="rounded px-1.5 py-0.5 text-2xs font-medium text-warning-foreground hover:bg-warning-surface disabled:opacity-50 dark:hover:bg-warning-surface/40"
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
          <h3 className="mb-1 text-2xs font-semibold uppercase tracking-wider text-warning-foreground">
            {t("deferredLabel")}
            <span className="ml-1 font-mono text-muted-foreground">
              ({deferred.length})
            </span>
          </h3>
          <ul className="space-y-0.5">
            {deferred.map((d) => (
              <li
                key={d.table}
                className="flex items-center gap-2 rounded px-1.5 py-1 hover:bg-surface-raised"
              >
                <div className="min-w-0 flex-1">
                  <p className="truncate font-mono text-[11px]">{d.table}</p>
                  <p className="truncate text-2xs italic text-muted-foreground">
                    {d.reason}
                  </p>
                </div>
                <button
                  type="button"
                  onClick={() => handlePromote(d.table)}
                  disabled={busy}
                  className="rounded px-1.5 py-0.5 text-2xs font-medium text-brand-foreground hover:bg-brand-surface disabled:opacity-50-strong dark:hover:bg-brand-surface/40"
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
        className="w-32 rounded border border-warning-border bg-surface-base px-1.5 py-0.5 text-2xs focus:border-warning-border focus:outline-none"
      />
      <button
        type="button"
        onClick={onConfirm}
        disabled={busy || reason.trim() === ""}
        className="rounded bg-warning-foreground px-1.5 py-0.5 text-2xs font-medium text-white hover:bg-warning-foreground disabled:opacity-50"
      >
        {t("actions.deferConfirm")}
      </button>
      <button
        type="button"
        onClick={onCancel}
        disabled={busy}
        className="rounded px-1 py-0.5 text-2xs text-muted-foreground hover:bg-surface-inset"
        aria-label={t("actions.cancel")}
      >
        ✕
      </button>
    </div>
  );
}
