"use client";

// Glossary term form — used by both the create dialog and the
// inline edit row of `/settings/glossary`. Shape mirrors
// `ox_ontology::glossary::GlossaryTermDef` minus the auto-generated
// `id` (the form lets the user supply one or omits it for the
// server to fill on Create — same contract as the rest of the
// admin-CRUD surface).

import { useState, type FormEvent } from "react";
import { useTranslations } from "next-intl";

import { Button } from "@/components/ui/button";
import {
  SettingsInput,
  SettingsTextarea,
} from "@/components/ui/form-input";
import type { GlossaryTermDef } from "@/lib/api/edit-ops";

interface GlossaryFormProps {
  /** Initial values when editing an existing term; `undefined`
   *  produces a blank create form. */
  initial?: GlossaryTermDef;
  /** Called with the (canonical-shape) GlossaryTermDef when the
   *  user submits. The id field stays blank for create flows;
   *  callers fill it before constructing the edit op. */
  onSubmit: (def: GlossaryTermDef) => void;
  /** Cancel button handler — closes the dialog or exits edit mode. */
  onCancel: () => void;
  /** Disable the submit button — callers pass the mutation's
   *  `isPending` so the form locks during the round-trip. */
  pending?: boolean;
}

export function GlossaryForm({
  initial,
  onSubmit,
  onCancel,
  pending = false,
}: GlossaryFormProps) {
  const t = useTranslations("settings.vocabulary.glossary.form");
  // Initial values are seeded from `initial` once. Callers who need
  // to bind the form to a different term re-mount it with a new
  // `key` prop (per React 19's `react-hooks/set-state-in-effect`
  // recommendation) rather than relying on a state-resetting effect.
  const [term, setTerm] = useState(initial?.term ?? "");
  const [displayDefault, setDisplayDefault] = useState(
    initial?.display_name?.default ?? "",
  );
  const [descriptionDefault, setDescriptionDefault] = useState(
    initial?.description?.default ?? "",
  );
  const [aliasesText, setAliasesText] = useState(
    initial?.aliases?.join(", ") ?? "",
  );
  const [category, setCategory] = useState(initial?.category ?? "");

  const handleSubmit = (e: FormEvent) => {
    e.preventDefault();
    if (!term.trim()) return;
    const aliases = aliasesText
      .split(",")
      .map((a) => a.trim())
      .filter(Boolean);
    onSubmit({
      // Preserve the existing id when editing; create flow leaves
      // it blank and the page fills it (UUIDv7) before posting.
      id: initial?.id ?? "",
      term: term.trim(),
      display_name: displayDefault
        ? { default: displayDefault, locales: initial?.display_name?.locales }
        : undefined,
      description: descriptionDefault
        ? { default: descriptionDefault, locales: initial?.description?.locales }
        : undefined,
      aliases,
      category: category.trim() || null,
      parent_term_id: initial?.parent_term_id ?? null,
    });
  };

  const submitLabel = initial ? t("submitUpdate") : t("submitCreate");

  return (
    <form onSubmit={handleSubmit} className="flex flex-col gap-3">
      <SettingsInput
        label={t("term")}
        value={term}
        onChange={(e) => setTerm(e.target.value)}
        placeholder={t("termPlaceholder")}
        required
      />
      <SettingsInput
        label={t("displayName")}
        value={displayDefault}
        onChange={(e) => setDisplayDefault(e.target.value)}
        placeholder={t("displayNamePlaceholder")}
      />
      <SettingsTextarea
        label={t("description")}
        value={descriptionDefault}
        onChange={(e) => setDescriptionDefault(e.target.value)}
        placeholder={t("descriptionPlaceholder")}
        rows={3}
      />
      <SettingsInput
        label={t("aliases")}
        value={aliasesText}
        onChange={(e) => setAliasesText(e.target.value)}
        placeholder={t("aliasesPlaceholder")}
      />
      <SettingsInput
        label={t("category")}
        value={category}
        onChange={(e) => setCategory(e.target.value)}
        placeholder={t("categoryPlaceholder")}
      />
      <div className="mt-1 flex items-center justify-end gap-2">
        <Button
          type="button"
          variant="ghost"
          size="sm"
          onClick={onCancel}
          disabled={pending}
        >
          {t("cancel")}
        </Button>
        <Button
          type="submit"
          size="sm"
          disabled={pending || !term.trim()}
        >
          {submitLabel}
        </Button>
      </div>
    </form>
  );
}
