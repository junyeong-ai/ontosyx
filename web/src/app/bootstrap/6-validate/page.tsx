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
  type GlossaryTermDraft,
} from "@/lib/api/bootstrap";
import {
  createOntology,
  findOntologyByName,
  type OntologyEditOp,
} from "@/lib/api/ontology";
import { createProject } from "@/lib/api/projects";
import type {
  AnalyzeSelection,
  CreateProjectRequest,
  OntologyListItem,
} from "@/types/api";

import { useBootstrap } from "../bootstrap-state";
import { bootstrapSourceToProjectSource } from "../source-mapping";
import { StepShell } from "../step-shell";
import {
  ExistingPilotDialog,
  suggestRename,
  type ExistingPilotChoice,
} from "./existing-pilot-dialog";

/**
 * Convert a parsed glossary draft row to a `CreateGlossaryTerm` op.
 * Trimmed text + filtered aliases; fresh UUID for the id so the
 * server can return a stable handle on the new term even before
 * the wizard knows it.
 */
function glossaryDraftToCreateOp(draft: GlossaryTermDraft): OntologyEditOp {
  const description = draft.description?.trim();
  return {
    op: "create_glossary_term",
    def: {
      id: crypto.randomUUID(),
      term: { default: draft.term, translations: {} },
      ...(description && description.length > 0
        ? { description: { default: description, translations: {} } }
        : {}),
      aliases: draft.aliases
        .filter((a) => a.length > 0)
        .map((a) => ({ default: a, translations: {} })),
    },
  };
}

/**
 * Map a wizard source kind + connection string + the step-2b table
 * picker state to a `CreateProjectRequest`, or `null` when the pair
 * can't be materialised without extra user input.
 *
 * `analyzeMode = "subset"` lowers to `selection: { kind: "subset",
 * tables }`; `"all"` lowers to `selection: { kind: "all" }` (which
 * is also the server default — sent explicitly so the wire payload
 * is self-describing).
 */
function buildCreateRequest(
  pilotName: string,
  sourceKind: string,
  sourceConnection: string,
  analyzeMode: "all" | "subset" | "staged",
  selectedTables: string[],
): CreateProjectRequest | null {
  const title = pilotName.trim() || undefined;
  const source = bootstrapSourceToProjectSource(sourceKind, sourceConnection);
  if (!source) return null;
  const selection: AnalyzeSelection =
    analyzeMode === "subset"
      ? { kind: "subset", tables: selectedTables }
      : analyzeMode === "staged"
        ? { kind: "staged", tables: selectedTables }
        : { kind: "all" };
  return { title, origin_type: "source", source, selection };
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
   * Persist the glossary draft as a bootstrap ontology. Each parsed
   * row becomes a `CreateGlossaryTerm` op; the whole batch commits
   * atomically as v1 via the unified `POST /api/ontologies`
   * endpoint. A failure surfaces as a toast but never rolls back
   * the downstream project creation — the drafts stay in
   * localStorage so the user can retry.
   *
   * Returns the new ontology's id on success so we can deep-link
   * the user to the Complete Map after Finish.
   */
  const seedGlossaryIfNeeded = async (): Promise<string | null> => {
    const drafts = parseGlossaryDraft(state.glossaryDraft);
    if (drafts.length === 0) return null;
    const name =
      state.pilotName.trim() ||
      `Bootstrap ${new Date().toISOString().slice(0, 10)}`;
    try {
      const resp = await createOntology({
        name,
        description: state.pilotScope.trim() || undefined,
        initial_operations: drafts.map(glossaryDraftToCreateOp),
        message: "Seeded via Bootstrap wizard",
      });
      toast.success(
        t("toast.glossarySeeded", { count: resp.applied_operations }),
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
      state.analyzeMode,
      state.selectedTables,
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
    // Project ownership transferred to the server — the operator's
    // wizard state has done its job. Drop the localStorage entry so
    // returning to /bootstrap shows a clean slate instead of replaying
    // the previous session's pilot configuration.
    reset();
    router.push(`/design?project=${encodeURIComponent(project.id)}`);
  };

  const onFinish = async () => {
    markComplete("6-validate");
    setSubmitting(true);
    try {
      // Pre-flight name-collision check. Narrows the window on the
      // create POST's 409 path — the race-fallback still lives in
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
      <div className="rounded-lg border border-concept-border bg-concept-surface p-5">
        <h3 className="mb-3 text-sm font-semibold text-concept-foreground">
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
        <p className="mt-4 text-[11px] text-concept-foreground">
          {t("summary.nextStepHint")}
        </p>
      </div>

      <div className="mt-4 flex items-center gap-2 text-xs">
        <button
          type="button"
          onClick={handleRestart}
          className="rounded px-3 py-1.5 text-muted-foreground hover:bg-surface-inset"
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
      <dt className="font-medium text-concept-foreground">{label}</dt>
      <dd className="text-concept-foreground">{value}</dd>
    </>
  );
}
