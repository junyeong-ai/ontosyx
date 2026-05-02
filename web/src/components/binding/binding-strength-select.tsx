"use client";

// ---------------------------------------------------------------------------
// BindingStrengthSelect — FHIR R5-aligned strength picker for a
// `PropertyBinding`. Each `BindingTarget` kind has a different
// enforcement story, so the same control adapts:
//
//   value_set / code_system → all four strengths are meaningful
//                              (Required actually rejects writes
//                               via the SHACL derived rule).
//   notation_pattern        → Required + Preferred only
//                              (Extensible / Example don't fit a
//                               structural format constraint).
//   value_range             → strength is informational; the IR
//                              treats ranges as classifiers, not
//                              rejectors. Render disabled.
//   glossary                → strength has no enforcement semantics;
//                              the binding is a semantic anchor
//                              ("this property realises this
//                              concept"). Render disabled.
//
// Disabled-with-explanation is preferred over hide so the platform's
// behaviour stays discoverable. The disabled reason is wired through
// `aria-describedby` (visually hidden but announced by screen readers)
// — `title` alone is unreliable for assistive tech (WCAG 2.1).
// ---------------------------------------------------------------------------

import { useId } from "react";
import { useTranslations } from "next-intl";

import type { BindingStrength, PropertyBinding } from "@/types/ontology";

type BindingKind = PropertyBinding["kind"];

const ALL_STRENGTHS: BindingStrength[] = [
  "required",
  "extensible",
  "preferred",
  "example",
];

const PATTERN_STRENGTHS: BindingStrength[] = ["required", "preferred"];

interface Props {
  /** Discriminator from the binding's target — drives which
   *  strength options apply and whether the control is editable. */
  targetKind: BindingKind;
  value: BindingStrength;
  onChange: (next: BindingStrength) => void;
  /** Render the label inline above the control. The inline popover
   *  hides labels for density; the batch form shows them. */
  showLabel?: boolean;
  className?: string;
}

interface Policy {
  options: BindingStrength[];
  disabledReasonKey: string | null;
}

function policyFor(kind: BindingKind): Policy {
  switch (kind) {
    case "value_set":
    case "code_system":
      return { options: ALL_STRENGTHS, disabledReasonKey: null };
    case "notation_pattern":
      return { options: PATTERN_STRENGTHS, disabledReasonKey: null };
    case "value_range":
      return {
        options: ALL_STRENGTHS,
        disabledReasonKey: "disabled.valueRange",
      };
    case "glossary":
      return {
        options: ALL_STRENGTHS,
        disabledReasonKey: "disabled.glossary",
      };
  }
}

export function BindingStrengthSelect({
  targetKind,
  value,
  onChange,
  showLabel = true,
  className,
}: Props) {
  const t = useTranslations("binding.strength");
  const policy = policyFor(targetKind);
  const disabled = policy.disabledReasonKey !== null;
  const reason = policy.disabledReasonKey
    ? t(policy.disabledReasonKey)
    : undefined;

  // useId() yields a stable, mount-unique string so two instances on
  // the same page (inspector popover + batch panel side-by-side) do
  // not collide on `for`/`id` and `aria-describedby` references.
  const reactId = useId();
  const selectId = `binding-strength-${reactId}`;
  const reasonId = `binding-strength-reason-${reactId}`;

  return (
    <div className={className ?? "flex flex-col gap-1"}>
      {showLabel && (
        <label
          htmlFor={selectId}
          className="text-2xs font-medium uppercase tracking-wider text-muted-foreground"
        >
          {t("label")}
        </label>
      )}
      <select
        id={selectId}
        value={value}
        onChange={(e) => onChange(e.target.value as BindingStrength)}
        disabled={disabled}
        aria-describedby={reason ? reasonId : undefined}
        className="rounded border border-divider bg-surface-base px-2 py-1 text-xs disabled:cursor-not-allowed disabled:opacity-60"
      >
        {policy.options.map((s) => (
          <option key={s} value={s}>
            {t(`option.${s}`)}
          </option>
        ))}
      </select>
      {reason && (
        // Visually hidden but exposed to assistive tech via
        // `aria-describedby`. `sr-only` is the project's existing
        // utility for this (Tailwind default).
        <span id={reasonId} className="sr-only">
          {reason}
        </span>
      )}
    </div>
  );
}
