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
import { useTranslations } from "next-intl";
import { toast } from "sonner";

import { Modal } from "@/components/ui/modal";
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
    <Modal
      open={open}
      onOpenChange={onOpenChange}
      title={t("title")}
      description={t("subtitle")}
      size="md"
    >
      <form onSubmit={handleSubmit} className="space-y-3">
        <label className="block text-xs font-medium text-foreground">
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

        <label className="block text-xs font-medium text-foreground">
          {t("descriptionLabel")}
          <FormTextarea
            value={description}
            onChange={(e) => setDescription(e.target.value)}
            placeholder={t("descriptionPlaceholder")}
            rows={3}
            className="mt-1"
          />
        </label>

        <label className="block text-xs font-medium text-foreground">
          {t("tagsLabel")}
          <FormInput
            type="text"
            value={tagsInput}
            onChange={(e) => setTagsInput(e.target.value)}
            placeholder={t("tagsPlaceholder")}
            className="mt-1"
          />
        </label>

        <label className="block text-xs font-medium text-foreground">
          {t("conceptAnchorsLabel")}
          <FormInput
            type="text"
            value={conceptAnchorsInput}
            onChange={(e) => setConceptAnchorsInput(e.target.value)}
            placeholder={t("conceptAnchorsPlaceholder")}
            className="mt-1"
          />
          <span className="mt-1 block text-2xs text-foreground-muted">
            {t("conceptAnchorsHint")}
          </span>
        </label>

        <div className="flex justify-end gap-2 pt-2">
          <Button
            type="button"
            variant="ghost"
            size="sm"
            onClick={() => onOpenChange(false)}
          >
            {t("cancel")}
          </Button>
          <Button type="submit" size="sm" disabled={create.isPending}>
            {create.isPending ? t("saving") : t("save")}
          </Button>
        </div>
      </form>
    </Modal>
  );
}
