"use client";

// `useFormWithSchema` — zod-validate-on-submit, no react-hook-form.
//
// The workbench has ~12 forms today; every one of them implements
// the same flow:
//
//   1. Hand-roll local `useState` for each field.
//   2. In `handleSubmit`, run a hand-rolled `if (!field.trim()) ...`
//      ladder, set field-level error string, abort.
//   3. On success, clear errors, call `onSubmit(value)`.
//
// That ladder duplicates effort, drifts shape from form to form, and
// re-validation hooks (re-validate after the user fixes the typo)
// rarely make it in. This hook owns that flow:
//
//   * Caller passes a zod schema and an `onValid` handler.
//   * The hook returns `errors`, `submit`, and `clearErrors`.
//   * `submit(formValue)` runs `schema.safeParse`, sets per-field
//     `errors[path]` from the issue list, and either fires `onValid`
//     or surfaces the errors.
//   * `clearErrors(path?)` lets the caller wipe a single field's
//     error on change so re-validation happens implicitly: the user
//     types in a previously-erroring field, the error clears, and
//     the next submit attempt either succeeds or surfaces a fresh
//     error.
//
// Field state itself stays with the caller (still `useState` per
// field) — the hook only owns validation. That keeps consumers free
// to localise complex computed fields (alias arrays, lifecycle
// discriminated unions, etc.) without contorting them into the
// hook's internal model.

import { useCallback, useState } from "react";
import type { z } from "zod";

export type FieldErrors = Record<string, string>;

export interface FormWithSchema<TInput> {
  /** Field-keyed validation errors from the most recent `submit`. */
  errors: FieldErrors;
  /**
   * Validate `value` against `schema`. If valid, call `onValid` with
   * the parsed output and clear errors. If invalid, set `errors` and
   * return `false` so the caller can short-circuit the network call.
   */
  submit: (value: TInput) => boolean | Promise<boolean>;
  /** Drop a single field's error or every error if no path is given. */
  clearErrors: (path?: string) => void;
  /** True while an async `onValid` callback is in flight. */
  pending: boolean;
}

interface UseFormWithSchemaOptions<TInput, TOutput> {
  schema: z.ZodType<TOutput, TInput>;
  /**
   * Called with the parsed value when validation succeeds. If async,
   * `pending` flips true while it resolves so the caller's submit
   * button can disable / spinner without owning the lifecycle.
   */
  onValid: (value: TOutput) => void | Promise<void>;
}

function flattenZodIssues(error: z.ZodError): FieldErrors {
  const errors: FieldErrors = {};
  for (const issue of error.issues) {
    // The path can be empty (top-level error). Encode `""` as a
    // synthetic `_form` key so the consumer can render banner-style
    // form-wide errors without conflating with field-keyed ones.
    const key =
      issue.path.length === 0 ? "_form" : issue.path.map(String).join(".");
    if (!(key in errors)) {
      errors[key] = issue.message;
    }
  }
  return errors;
}

export function useFormWithSchema<TInput, TOutput>(
  options: UseFormWithSchemaOptions<TInput, TOutput>,
): FormWithSchema<TInput> {
  const [errors, setErrors] = useState<FieldErrors>({});
  const [pending, setPending] = useState(false);

  const submit = useCallback(
    async (value: TInput) => {
      const parsed = options.schema.safeParse(value);
      if (!parsed.success) {
        setErrors(flattenZodIssues(parsed.error));
        return false;
      }
      setErrors({});
      const out = options.onValid(parsed.data);
      if (out instanceof Promise) {
        setPending(true);
        try {
          await out;
        } finally {
          setPending(false);
        }
      }
      return true;
    },
    [options],
  );

  const clearErrors = useCallback((path?: string) => {
    if (path === undefined) {
      setErrors({});
      return;
    }
    setErrors((prev) => {
      if (!(path in prev)) return prev;
      const next = { ...prev };
      delete next[path];
      return next;
    });
  }, []);

  return { errors, submit, clearErrors, pending };
}
