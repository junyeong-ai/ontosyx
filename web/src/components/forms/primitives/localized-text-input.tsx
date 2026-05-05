"use client";

import { useTranslations } from "next-intl";

import { Button } from "@/components/ui/button";
import { FormInput } from "@/components/ui/form-input";
import type { LocalizedText } from "@/types/ontology";

// LocalizedTextInput — composite editor for the LocalizedText wire
// shape. The canonical `default` string is the primary input; per-
// locale translations are added as collapsible rows the operator can
// remove inline. Empty translation maps elide on commit so the JSON
// view stays tidy when no translations are authored.

interface LocalizedTextInputProps {
  /** Current value. `undefined` is treated as an empty
   *  `{ default: "" }` and the canonical row stays blank. */
  value: LocalizedText | undefined;
  onChange: (next: LocalizedText) => void;
  placeholder?: string;
  /** When true, the canonical row reads as a textarea (multiline) —
   *  used for description fields whose copy spans multiple sentences. */
  multiline?: boolean;
  disabled?: boolean;
  ariaInvalid?: boolean;
}

export function LocalizedTextInput({
  value,
  onChange,
  placeholder,
  multiline = false,
  disabled,
  ariaInvalid,
}: LocalizedTextInputProps) {
  const t = useTranslations("forms.localizedText");
  const current: LocalizedText = value ?? { default: "" };
  const translations = current.translations ?? {};
  const locales = Object.keys(translations);

  const updateDefault = (next: string) => {
    onChange({ ...current, default: next });
  };

  const updateTranslation = (locale: string, next: string) => {
    onChange({
      ...current,
      translations: { ...translations, [locale]: next },
    });
  };

  const renameTranslation = (oldLocale: string, newLocale: string) => {
    const trimmed = newLocale.trim();
    if (!trimmed || oldLocale === trimmed) return;
    const value = translations[oldLocale] ?? "";
    const next: Record<string, string> = { ...translations };
    delete next[oldLocale];
    next[trimmed] = value;
    onChange({ ...current, translations: next });
  };

  const removeTranslation = (locale: string) => {
    const next = { ...translations };
    delete next[locale];
    onChange({
      ...current,
      translations: Object.keys(next).length ? next : undefined,
    });
  };

  const addTranslation = () => {
    const seed = ["en", "ja", "zh"].find((l) => !translations[l]) ?? "xx";
    onChange({
      ...current,
      translations: { ...translations, [seed]: "" },
    });
  };

  return (
    <div className="flex flex-col gap-2">
      {multiline ? (
        <textarea
          value={current.default}
          onChange={(e) => updateDefault(e.target.value)}
          placeholder={placeholder ?? t("defaultPlaceholder")}
          disabled={disabled}
          aria-invalid={ariaInvalid || undefined}
          rows={3}
          className="w-full rounded-md border border-divider bg-surface-base px-2 py-1.5 text-xs text-foreground transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)] focus:border-brand-foreground focus:outline-none focus:ring-2 focus:ring-ring-default disabled:opacity-(--opacity-disabled) aria-invalid:border-danger-border aria-invalid:focus:ring-ring-danger"
        />
      ) : (
        <FormInput
          value={current.default}
          onChange={(e) => updateDefault(e.target.value)}
          placeholder={placeholder ?? t("defaultPlaceholder")}
          density="compact"
          disabled={disabled}
          aria-invalid={ariaInvalid}
        />
      )}

      {locales.map((locale) => (
        <div
          key={locale}
          className="flex items-center gap-2 rounded-md bg-surface-inset px-2 py-1.5"
        >
          <FormInput
            value={locale}
            onChange={(e) => renameTranslation(locale, e.target.value)}
            placeholder={t("localePlaceholder")}
            density="compact"
            disabled={disabled}
            className="w-16"
            aria-label={t("localeLabel")}
          />
          <FormInput
            value={translations[locale] ?? ""}
            onChange={(e) => updateTranslation(locale, e.target.value)}
            placeholder={t("translationPlaceholder", { locale })}
            density="compact"
            disabled={disabled}
            className="flex-1"
            aria-label={t("translationLabel", { locale })}
          />
          <Button
            type="button"
            variant="ghost"
            size="xs"
            onClick={() => removeTranslation(locale)}
            disabled={disabled}
            aria-label={t("removeAria", { locale })}
          >
            {t("remove")}
          </Button>
        </div>
      ))}

      <Button
        type="button"
        variant="ghost"
        size="xs"
        onClick={addTranslation}
        disabled={disabled}
        className="self-start"
      >
        {t("addTranslation")}
      </Button>
    </div>
  );
}
