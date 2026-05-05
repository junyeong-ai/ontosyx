"use client";

// `<TypeToConfirmField>` — phrase-match gate for high-stakes flows.
//
// The pattern: a destructive action (delete project, drop ontology
// version, transfer workspace ownership) is gated on the user
// typing a verbatim phrase — typically the resource name. The
// gate is what makes Foundry / Linear / GitHub feel safe; without
// it, "delete" is too easy to autocomplete past while distracted.
//
// `<ConfirmProvider>`'s `typeToConfirm` option already implements
// this for modal flows. This standalone primitive surfaces the
// same idiom for inline forms — settings pages that want to gate
// "transfer this workspace" behind name confirmation, vocabulary
// admin who renames a value-set with type-name verification, etc.
//
// The match is case-sensitive on purpose. Case-insensitive match
// makes "delete" too easy to autocomplete past — every modern
// platform that ships this idiom (GitHub, Stripe, Cloudflare,
// Linear) requires an exact match.

import { forwardRef, useId, type InputHTMLAttributes } from "react";

import { cn } from "@/lib/cn";
import { FormInput } from "./form-input";

interface TypeToConfirmFieldProps
  extends Omit<InputHTMLAttributes<HTMLInputElement>, "value" | "onChange"> {
  /** The exact phrase the user must type. Case-sensitive. */
  phrase: string;
  /** Current input value, lifted by the parent form. */
  value: string;
  onChange: (value: string) => void;
  /** Field label; rendered above the input. */
  label: string;
  /**
   * Show the phrase next to the label so the user can tell what to
   * type. Default true — flip to false only when the surrounding
   * form copy already names the phrase (e.g. "Type the project
   * name to confirm" reads cleanly without a `(my-project)` echo).
   */
  showPhrase?: boolean;
  /** Optional helper text below the input. */
  hint?: string;
  className?: string;
}

/**
 * Lifted-state input — the parent stays in control of the typed
 * value (so the form can display the gate's match status next to a
 * confirm button) and the field renders the visible chrome. The
 * `matchesPhrase` helper below is the parent's predicate; this
 * component does NOT internally compute "match" because then the
 * parent would have to re-derive it for the confirm button.
 */
export const TypeToConfirmField = forwardRef<
  HTMLInputElement,
  TypeToConfirmFieldProps
>(
  (
    {
      phrase,
      value,
      onChange,
      label,
      showPhrase = true,
      hint,
      className,
      ...rest
    },
    ref,
  ) => {
    const reactId = useId();
    const inputId = rest.id ?? `type-to-confirm-${reactId}`;
    const hintId = hint ? `${inputId}-hint` : undefined;
    return (
      <div className={cn("flex flex-col gap-1.5", className)}>
        <label htmlFor={inputId} className="text-xs">
          <span className="font-medium text-foreground-strong">{label}</span>
          {showPhrase && (
            <span className="ms-1 font-mono text-foreground-muted">
              ({phrase})
            </span>
          )}
        </label>
        <FormInput
          ref={ref}
          id={inputId}
          type="text"
          value={value}
          onChange={(e) => onChange(e.target.value)}
          spellCheck={false}
          autoComplete="off"
          aria-describedby={hintId}
          className="font-mono"
          {...rest}
        />
        {hint && (
          <p id={hintId} className="text-2xs text-foreground-muted">
            {hint}
          </p>
        )}
      </div>
    );
  },
);

TypeToConfirmField.displayName = "TypeToConfirmField";

/** Predicate the parent form pairs with the field. Case-sensitive. */
export function matchesConfirmPhrase(value: string, phrase: string): boolean {
  return value === phrase;
}
