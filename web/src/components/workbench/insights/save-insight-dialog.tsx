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

import { useCallback, useState } from "react";
import { useTranslations } from "next-intl";
import { z } from "zod";
import { toast } from "@/components/ui/toast";

import { Modal } from "@/components/ui/modal";
import { Button } from "@/components/ui/button";
import { FormField } from "@/components/ui/form-field";
import { FormInput, FormTextarea } from "@/components/ui/form-input";
import { useCreateInsight } from "@/hooks/api/use-insights";
import { useFormWithSchema } from "@/hooks/use-form-with-schema";
import type { InsightProvenance, InsightQueryIR } from "@/types/api";

interface Props {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** The QueryIR to persist — server validates it again. */
  queryIr: InsightQueryIR;
  /** Optional provenance from the most recent execution. */
  originalProvenance?: InsightProvenance;
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

const SCHEMA = z.object({
  question: z.string().trim().min(1, { message: "errors.questionRequired" }),
});

type SaveInsightFormInput = z.input<typeof SCHEMA>;

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

  const onValid = useCallback(
    async (value: SaveInsightFormInput) => {
      try {
        await create.mutateAsync({
          // Wire shape mirrors `ox_query_ir::insight::CreateInsightRequest`:
          // `description` is `LocalizedText` non-Option on the canonical
          // `InsightDef`, but the request lets the client omit it; the
          // server defaults to an empty `LocalizedText`. Sending a present-
          // but-empty record from the client keeps the server contract
          // honest without relying on that defaulting behaviour.
          question: { default: value.question },
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
    },
    [
      create,
      description,
      tagsInput,
      conceptAnchorsInput,
      onOpenChange,
      originalProvenance,
      queryIr,
      t,
    ],
  );

  const { errors, submit, clearErrors, pending } = useFormWithSchema({
    schema: SCHEMA,
    onValid,
  });

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    void submit({ question });
  };

  const questionError = errors.question ? t(errors.question) : undefined;

  return (
    <Modal
      open={open}
      onOpenChange={onOpenChange}
      title={t("title")}
      description={t("subtitle")}
      size="md"
    >
      <form onSubmit={handleSubmit} className="space-y-3">
        <FormField label={t("questionLabel")} error={questionError}>
          <FormInput
            type="text"
            value={question}
            onChange={(e) => {
              setQuestion(e.target.value);
              clearErrors("question");
            }}
            placeholder={t("questionPlaceholder")}
            autoFocus
            error={!!questionError}
          />
        </FormField>

        <FormField label={t("descriptionLabel")}>
          <FormTextarea
            value={description}
            onChange={(e) => setDescription(e.target.value)}
            placeholder={t("descriptionPlaceholder")}
            rows={3}
          />
        </FormField>

        <FormField label={t("tagsLabel")}>
          <FormInput
            type="text"
            value={tagsInput}
            onChange={(e) => setTagsInput(e.target.value)}
            placeholder={t("tagsPlaceholder")}
          />
        </FormField>

        <FormField
          label={t("conceptAnchorsLabel")}
          hint={t("conceptAnchorsHint")}
        >
          <FormInput
            type="text"
            value={conceptAnchorsInput}
            onChange={(e) => setConceptAnchorsInput(e.target.value)}
            placeholder={t("conceptAnchorsPlaceholder")}
          />
        </FormField>

        <div className="flex justify-end gap-2 pt-2">
          <Button
            type="button"
            variant="ghost"
            size="sm"
            onClick={() => onOpenChange(false)}
          >
            {t("cancel")}
          </Button>
          <Button type="submit" size="sm" disabled={pending}>
            {pending ? t("saving") : t("save")}
          </Button>
        </div>
      </form>
    </Modal>
  );
}
