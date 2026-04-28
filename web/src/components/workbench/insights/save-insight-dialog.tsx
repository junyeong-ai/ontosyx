"use client";

// ---------------------------------------------------------------------------
// SaveInsightDialog — turns the current Analyze result into a
// persisted Insight artefact. The query IR + provenance are passed
// in by the caller (Analyze panel owns them); the user fills in
// question text, description, and tags.
//
// Built on `@base-ui/react/dialog` so escape / focus-trap / backdrop-
// click semantics match every other modal in the project (cf.
// `app/settings/glossary/page.tsx`, `components/ui/prompt-dialog.tsx`).
// ---------------------------------------------------------------------------

import { useState } from "react";
import { Dialog } from "@base-ui/react/dialog";
import { useTranslations } from "next-intl";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import { FormInput } from "@/components/ui/form-input";
import { FormTextarea } from "@/components/ui/form-textarea";
import { useCreateInsight } from "@/hooks/api/use-insights";

interface Props {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** The QueryIR to persist — server validates it again. */
  queryIr: unknown;
  /** Optional provenance from the most recent execution. */
  originalProvenance?: unknown;
  /** Pre-filled question text (e.g. the Analyze chat prompt). */
  defaultQuestion?: string;
}

/** Split a comma-separated list into trimmed, non-empty tokens. */
function splitCsv(raw: string): string[] {
  return raw
    .split(",")
    .map((s) => s.trim())
    .filter(Boolean);
}

export function SaveInsightDialog({
  open,
  onOpenChange,
  queryIr,
  originalProvenance,
  defaultQuestion,
}: Props) {
  const t = useTranslations("workbench.insights.save");
  const [question, setQuestion] = useState(defaultQuestion ?? "");
  const [description, setDescription] = useState("");
  const [tagsInput, setTagsInput] = useState("");
  const [conceptAnchorsInput, setConceptAnchorsInput] = useState("");
  const create = useCreateInsight();

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!question.trim()) {
      toast.error(t("questionRequired"));
      return;
    }
    try {
      await create.mutateAsync({
        question: { default: question.trim() },
        // Wire shape mirrors `ox_query_ir::insight::CreateInsightRequest`:
        // `description` is `LocalizedText` non-Option on the canonical
        // `InsightDef`, but the request lets the client omit it; the
        // server defaults to an empty `LocalizedText`. Sending a present-
        // but-empty record from the client keeps the server contract
        // honest without relying on that defaulting behaviour.
        description: { default: description.trim() },
        tags: splitCsv(tagsInput),
        concept_anchors: splitCsv(conceptAnchorsInput),
        query_ir: queryIr,
        original_provenance: originalProvenance,
      });
      toast.success(t("saveSuccess"));
      setQuestion("");
      setDescription("");
      setTagsInput("");
      setConceptAnchorsInput("");
      onOpenChange(false);
    } catch (err) {
      toast.error(err instanceof Error ? err.message : t("saveFailed"));
    }
  };

  return (
    <Dialog.Root open={open} onOpenChange={onOpenChange}>
      <Dialog.Portal>
        <Dialog.Backdrop className="fixed inset-0 z-50 bg-black/40 backdrop-blur-sm data-[starting-style]:opacity-0 data-[ending-style]:opacity-0 transition-opacity" />
        <Dialog.Popup className="fixed left-1/2 top-1/2 z-50 w-full max-w-md -translate-x-1/2 -translate-y-1/2 rounded-xl border border-zinc-200 bg-white p-6 shadow-xl data-[starting-style]:scale-95 data-[starting-style]:opacity-0 data-[ending-style]:scale-95 data-[ending-style]:opacity-0 transition-all dark:border-zinc-700 dark:bg-zinc-900">
          <Dialog.Title className="text-base font-semibold text-zinc-900 dark:text-zinc-100">
            {t("title")}
          </Dialog.Title>
          <Dialog.Description className="mt-1 text-xs text-zinc-500 dark:text-zinc-400">
            {t("subtitle")}
          </Dialog.Description>

          <form onSubmit={handleSubmit} className="mt-4 space-y-3">
            <label className="block text-xs font-medium text-zinc-700 dark:text-zinc-300">
              {t("questionLabel")}
              <FormInput
                type="text"
                value={question}
                onChange={(e) => setQuestion(e.target.value)}
                placeholder={t("questionPlaceholder")}
                className="mt-1"
                autoFocus
              />
            </label>

            <label className="block text-xs font-medium text-zinc-700 dark:text-zinc-300">
              {t("descriptionLabel")}
              <FormTextarea
                value={description}
                onChange={(e) => setDescription(e.target.value)}
                placeholder={t("descriptionPlaceholder")}
                rows={3}
                className="mt-1"
              />
            </label>

            <label className="block text-xs font-medium text-zinc-700 dark:text-zinc-300">
              {t("tagsLabel")}
              <FormInput
                type="text"
                value={tagsInput}
                onChange={(e) => setTagsInput(e.target.value)}
                placeholder={t("tagsPlaceholder")}
                className="mt-1"
              />
            </label>

            <label className="block text-xs font-medium text-zinc-700 dark:text-zinc-300">
              {t("conceptAnchorsLabel")}
              <FormInput
                type="text"
                value={conceptAnchorsInput}
                onChange={(e) => setConceptAnchorsInput(e.target.value)}
                placeholder={t("conceptAnchorsPlaceholder")}
                className="mt-1"
              />
              <span className="mt-1 block text-[10px] text-muted-foreground">
                {t("conceptAnchorsHint")}
              </span>
            </label>

            <div className="flex justify-end gap-2 pt-2">
              <Dialog.Close
                render={
                  <Button type="button" variant="ghost" size="sm">
                    {t("cancel")}
                  </Button>
                }
              />
              <Button type="submit" size="sm" disabled={create.isPending}>
                {create.isPending ? t("saving") : t("save")}
              </Button>
            </div>
          </form>
        </Dialog.Popup>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
