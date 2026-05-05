"use client";

import { useState } from "react";
import { useTranslations } from "next-intl";
import { Database } from "lucide-react";
import { Eyebrow } from "@/components/ui/eyebrow";
import { toast } from "@/components/ui/toast";

import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover";
import { FormInput } from "@/components/ui/form-input";
import { Button } from "@/components/ui/button";
import { useAppStore } from "@/lib/store";
import {
  useDeferScopeTables,
  useIncludeScopeTables,
} from "@/hooks/api/use-ontology-drafts";
import type { DeferredTable } from "@/types/ontology-drafts";

export function ScopeBadge() {
  const t = useTranslations("workbench.design.scope");
  const project = useAppStore((s) => s.activeOntologyDraft);
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
      <PopoverTrigger className="inline-flex items-center gap-1 rounded-md border border-brand-border bg-surface-base px-2 py-1 text-2xs text-brand-foreground shadow-1 hover:bg-brand-surface-strong">
        <Database className="h-3 w-3" />
        {summary}
      </PopoverTrigger>
      <PopoverContent className="z-popover w-inspector rounded-lg border border-divider bg-surface-base p-3 shadow-4 outline-none focus-visible:ring-0">
        <ScopePanel
          ontologyDraftId={project.id}
          revision={project.revision}
          included={included}
          deferred={deferred}
        />
      </PopoverContent>
    </Popover>
  );
}

function ScopePanel({
  ontologyDraftId,
  revision,
  included,
  deferred,
}: {
  ontologyDraftId: string;
  revision: number;
  included: string[];
  deferred: DeferredTable[];
}) {
  const t = useTranslations("workbench.design.scope");
  const include = useIncludeScopeTables(ontologyDraftId);
  const defer = useDeferScopeTables(ontologyDraftId);
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
          <Eyebrow level={3} tone="brand" className="mb-1">
            {t("includedLabel")}
            <span className="ms-1 font-mono text-foreground-muted">
              ({included.length})
            </span>
          </Eyebrow>
          <ul className="space-y-0.5">
            {included.map((table) => (
              <li
                key={table}
                className="flex items-center gap-2 rounded px-1.5 py-1 hover:bg-surface-raised"
              >
                <span className="flex-1 truncate font-mono text-2xs">
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
                    className="rounded px-1.5 py-0.5 text-2xs font-medium text-warning-foreground hover:bg-warning-surface disabled:opacity-50"
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
          <Eyebrow level={3} tone="warning" className="mb-1">
            {t("deferredLabel")}
            <span className="ms-1 font-mono text-foreground-muted">
              ({deferred.length})
            </span>
          </Eyebrow>
          <ul className="space-y-0.5">
            {deferred.map((d) => (
              <li
                key={d.table}
                className="flex items-center gap-2 rounded px-1.5 py-1 hover:bg-surface-raised"
              >
                <div className="min-w-0 flex-1">
                  <p className="truncate font-mono text-2xs">{d.table}</p>
                  <p className="truncate text-2xs italic text-foreground-muted">
                    {d.reason}
                  </p>
                </div>
                <button
                  type="button"
                  onClick={() => handlePromote(d.table)}
                  disabled={busy}
                  className="rounded px-1.5 py-0.5 text-2xs font-medium text-brand-foreground hover:bg-brand-surface disabled:opacity-50-strong"
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
      <FormInput
        autoFocus
        density="compact"
        value={reason}
        onChange={(e) => onChange(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter") onConfirm();
          else if (e.key === "Escape") onCancel();
        }}
        placeholder={t("deferReasonPlaceholder")}
        aria-label={t("deferReasonPlaceholder")}
        className="w-32 border-warning-border"
      />
      <Button
        variant="primary"
        size="xs"
        onClick={onConfirm}
        disabled={reason.trim() === ""}
        loading={busy}
        className="bg-warning-foreground hover:bg-warning-foreground"
      >
        {t("actions.deferConfirm")}
      </Button>
      <Button
        type="button"
        variant="ghost"
        size="xs"
        onClick={onCancel}
        disabled={busy}
        aria-label={t("actions.cancel")}
      >
        ✕
      </Button>
    </div>
  );
}
