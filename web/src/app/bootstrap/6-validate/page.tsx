"use client";

// Final step — shows a summary card and, on Finish, creates a
// DesignProject seeded from the wizard's captured intent. The
// request shape depends on sourceKind: for connection-string based
// sources (postgresql / mysql / bigquery) we pass the user-supplied
// connection; for file-based kinds (csv / json) we skip the
// backend call and just land on the workbench with the bootstrap
// state in localStorage for the workbench to pick up.

import { useRouter } from "next/navigation";
import { useTranslations } from "next-intl";
import { useMemo, useState } from "react";
import { toast } from "sonner";

import { createProject } from "@/lib/api/projects";
import type { CreateProjectRequest, DesignSource } from "@/types/api";

import { useBootstrap } from "../bootstrap-state";
import { StepShell } from "../step-shell";

/**
 * Map a wizard source kind + connection string to a
 * `CreateProjectRequest`, or `null` when the pair can't be
 * materialised without extra user input (file upload, etc.).
 */
function buildCreateRequest(
  pilotName: string,
  sourceKind: string,
  sourceConnection: string,
): CreateProjectRequest | null {
  const title = pilotName.trim() || undefined;
  const conn = sourceConnection.trim();
  if (!conn) return null;
  let source: DesignSource | null = null;
  switch (sourceKind) {
    case "postgresql":
      source = { type: "postgresql", connection_string: conn };
      break;
    case "mysql":
      // Mysql requires a schema — use `public` as the conservative
      // default and let the admin retarget from the workbench.
      source = { type: "mysql", connection_string: conn, schema: "public" };
      break;
    default:
      return null;
  }
  return { title, origin_type: "source", source };
}

export default function ValidateStep() {
  const t = useTranslations("bootstrap.step6");
  const router = useRouter();
  const { state, reset, markComplete } = useBootstrap();
  const [submitting, setSubmitting] = useState(false);

  const glossaryCount = useMemo(
    () => state.glossaryDraft.split("\n").filter((l) => l.trim().length > 0).length,
    [state.glossaryDraft],
  );

  const ruleCount = useMemo(
    () => state.rulesDraft.split("\n").filter((l) => l.trim().length > 0).length,
    [state.rulesDraft],
  );

  const onFinish = async () => {
    markComplete("6-validate");
    const req = buildCreateRequest(
      state.pilotName,
      state.sourceKind,
      state.sourceConnection,
    );
    if (!req) {
      // The wizard is still useful even without a DB source — land
      // on /design and leave the bootstrap state in localStorage
      // for the workbench to pick up.
      toast.info(t("toast.skippedCreate"));
      router.push("/design");
      return;
    }
    setSubmitting(true);
    try {
      const project = await createProject(req);
      toast.success(
        t("toast.created", { name: state.pilotName || t("summary.unnamed") }),
      );
      // The design workbench resolves the project by path slot; we
      // pass the id so the workbench opens focused on this project.
      router.push(`/design?project=${encodeURIComponent(project.id)}`);
    } catch (err) {
      toast.error(
        err instanceof Error
          ? t("toast.createFailed", { message: err.message })
          : t("toast.createFailedUnknown"),
      );
    } finally {
      setSubmitting(false);
    }
  };

  const handleRestart = () => {
    reset();
    router.push("/bootstrap/1-pilot");
  };

  return (
    <StepShell
      stepKey="6-validate"
      nextPath={null}
      backPath="/bootstrap/5-map"
      canAdvance={!submitting}
      onFinish={onFinish}
      title={t("title")}
      subtitle={t("subtitle")}
    >
      <div className="rounded-lg border border-violet-200 bg-violet-50 p-5 dark:border-violet-900/50 dark:bg-violet-950/30">
        <h3 className="mb-3 text-sm font-semibold text-violet-900 dark:text-violet-200">
          {t("summary.title", { name: state.pilotName || t("summary.unnamed") })}
        </h3>
        <dl className="grid grid-cols-2 gap-x-4 gap-y-2 text-xs">
          <SummaryRow
            label={t("summary.scopeLabel")}
            value={state.pilotScope || t("summary.notSet")}
          />
          <SummaryRow
            label={t("summary.sourceLabel")}
            value={state.sourceKind || t("summary.notSet")}
          />
          <SummaryRow
            label={t("summary.glossaryLabel")}
            value={t("summary.count", { n: glossaryCount })}
          />
          <SummaryRow
            label={t("summary.rulesLabel")}
            value={t("summary.count", { n: ruleCount })}
          />
        </dl>
        <p className="mt-4 text-[11px] text-violet-700 dark:text-violet-300">
          {t("summary.nextStepHint")}
        </p>
      </div>

      <div className="mt-4 flex items-center gap-2 text-xs">
        <button
          type="button"
          onClick={handleRestart}
          className="rounded px-3 py-1.5 text-muted-foreground hover:bg-zinc-100 dark:hover:bg-zinc-800"
        >
          {t("restart")}
        </button>
      </div>
    </StepShell>
  );
}

function SummaryRow({ label, value }: { label: string; value: string }) {
  return (
    <>
      <dt className="font-medium text-violet-900 dark:text-violet-200">{label}</dt>
      <dd className="text-violet-700 dark:text-violet-300">{value}</dd>
    </>
  );
}
