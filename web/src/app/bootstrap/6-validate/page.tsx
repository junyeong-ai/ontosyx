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

import {
  parseGlossaryDraft,
  seedBootstrapGlossary,
} from "@/lib/api/bootstrap";
import { findOntologyByName } from "@/lib/api/ontology";
import { createProject } from "@/lib/api/projects";
import type {
  CreateProjectRequest,
  DesignSource,
  OntologyListItem,
} from "@/types/api";

import { useBootstrap } from "../bootstrap-state";
import { StepShell } from "../step-shell";
import {
  ExistingPilotDialog,
  suggestRename,
  type ExistingPilotChoice,
} from "./existing-pilot-dialog";

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
  const { state, reset, markComplete, update } = useBootstrap();
  const [submitting, setSubmitting] = useState(false);
  const [existing, setExisting] = useState<OntologyListItem | null>(null);

  const glossaryCount = useMemo(
    () => state.glossaryDraft.split("\n").filter((l) => l.trim().length > 0).length,
    [state.glossaryDraft],
  );

  const ruleCount = useMemo(
    () => state.rulesDraft.split("\n").filter((l) => l.trim().length > 0).length,
    [state.rulesDraft],
  );

  /**
   * Persist the glossary draft as a bootstrap ontology (one commit
   * containing every parsed term). Fire-and-report: a failure is
   * surfaced as a toast but never rolls back the project creation
   * below — the drafts stay in localStorage so the user can retry.
   *
   * Returns the new ontology's id on success so we can deep-link
   * the user to the Complete Map after Finish.
   */
  const seedGlossaryIfNeeded = async (): Promise<string | null> => {
    const terms = parseGlossaryDraft(state.glossaryDraft);
    if (terms.length === 0) return null;
    const name =
      state.pilotName.trim() ||
      `Bootstrap ${new Date().toISOString().slice(0, 10)}`;
    try {
      const resp = await seedBootstrapGlossary({
        name,
        description: state.pilotScope.trim() || undefined,
        terms,
      });
      toast.success(
        t("toast.glossarySeeded", { count: resp.committed_terms }),
      );
      return resp.ontology_id;
    } catch (err) {
      toast.error(
        err instanceof Error
          ? t("toast.glossarySeedFailed", { message: err.message })
          : t("toast.glossarySeedFailedUnknown"),
      );
      return null;
    }
  };

  /**
   * Continue the Finish pipeline once we've confirmed the pilot name
   * is safe to submit (either no collision, or the user has resolved
   * a collision via the dialog). Encodes the same path the legacy
   * direct-Finish used to walk.
   */
  const runFinishPipeline = async () => {
    const seededOntologyId = await seedGlossaryIfNeeded();

    const req = buildCreateRequest(
      state.pilotName,
      state.sourceKind,
      state.sourceConnection,
    );
    if (!req) {
      toast.info(t("toast.skippedCreate"));
      if (seededOntologyId) {
        router.push(
          `/ontology/${encodeURIComponent(seededOntologyId)}/map`,
        );
      } else {
        router.push("/design");
      }
      return;
    }
    const project = await createProject(req);
    toast.success(
      t("toast.created", { name: state.pilotName || t("summary.unnamed") }),
    );
    router.push(`/design?project=${encodeURIComponent(project.id)}`);
  };

  const onFinish = async () => {
    markComplete("6-validate");
    setSubmitting(true);
    try {
      // Pre-flight name-collision check. Narrows the window on
      // seed-glossary's 409 path — the race-fallback still lives in
      // seedGlossaryIfNeeded's catch, so a second user committing
      // between the lookup and the POST is still surfaced as a toast.
      const pilotName = state.pilotName.trim();
      if (pilotName) {
        const hit = await findOntologyByName(pilotName);
        if (hit) {
          // Park the pipeline; the dialog's choice handler
          // resumes it via the corresponding branch.
          setExisting(hit);
          setSubmitting(false);
          return;
        }
      }
      await runFinishPipeline();
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

  const handleExistingChoice = async (choice: ExistingPilotChoice) => {
    const target = existing;
    setExisting(null);
    if (!target) return;
    if (choice === "cancel") return;
    if (choice === "continue") {
      // Deep-link into the workbench for the existing pilot and
      // clear the wizard so the user doesn't re-enter the same
      // intent the next time they visit /bootstrap.
      reset();
      router.push(
        `/ontology/${encodeURIComponent(target.id)}/map`,
      );
      return;
    }
    // Rename — bump the pilot name to the suggested suffix, keep the
    // rest of the wizard state, and send the user back to step 1 so
    // they can confirm or edit the new name before advancing again.
    update({ pilotName: suggestRename(state.pilotName) });
    router.push("/bootstrap/1-pilot");
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

      <ExistingPilotDialog
        open={existing !== null}
        existing={existing}
        renameSuggestion={suggestRename(state.pilotName)}
        onChoose={handleExistingChoice}
      />
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
