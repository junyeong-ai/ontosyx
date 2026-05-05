"use client";

import { type FormEvent, useMemo, useState } from "react";
import { useTranslations } from "next-intl";

import { SaveBar } from "@/components/ui/save-bar";
import { snapshotEqual } from "@/lib/snapshot-equal";
import {
  type EntitySchema,
  type FieldError,
  validateRecord,
} from "@/lib/forms/field-schema";

import { StructuredForm } from "./structured-form";

// StructuredEntityEditor — wraps `StructuredForm` with the
// industry-standard sticky save bar pattern. Dirty state is derived
// from a deep comparison of the current `record` vs the initial
// snapshot; the save bar surfaces only when there are unsaved
// changes (Linear / Sanity / Notion / Stripe Dashboard pattern).
// Inline save / discard actions live in the bar; cancel is no
// longer first-class because the master-detail shell owns the
// pane lifecycle.

interface StructuredEntityEditorProps<T> {
  schema: EntitySchema<T>;
  /** Existing record when editing; `undefined` opens with
   *  `schema.buildDefault()`. */
  initial?: T;
  onSubmit: (record: T) => void;
  /** Optional cancel hook — used by create-draft flows that need to
   *  bail out without saving. Edit flows leave it unset. */
  onCancel?: () => void;
  pending?: boolean;
}

export function StructuredEntityEditor<T>({
  schema,
  initial,
  onSubmit,
  onCancel,
  pending = false,
}: StructuredEntityEditorProps<T>) {
  const t = useTranslations("forms");
  const initialOrDefault = useMemo(
    () => initial ?? schema.buildDefault(),
    [initial, schema],
  );
  const [record, setRecord] = useState<T>(initialOrDefault);
  const [submitted, setSubmitted] = useState(false);

  const dirty = useMemo(
    () => !snapshotEqual(record, initialOrDefault),
    [record, initialOrDefault],
  );

  const errors = useMemo(() => {
    if (!submitted) return new Map<string, FieldError>();
    const out = new Map<string, FieldError>();
    for (const err of validateRecord(schema, record)) {
      const key = (err.params?.field as string) ?? "_form";
      if (!out.has(key)) out.set(key, err);
    }
    return out;
  }, [schema, record, submitted]);

  const handleSubmit = (e: FormEvent) => {
    e.preventDefault();
    setSubmitted(true);
    const errorList = validateRecord(schema, record);
    if (errorList.length > 0) return;
    onSubmit(record);
  };

  const handleSave = () => {
    setSubmitted(true);
    const errorList = validateRecord(schema, record);
    if (errorList.length > 0) return;
    onSubmit(record);
  };

  const handleDiscard = () => {
    setRecord(initialOrDefault);
    setSubmitted(false);
    onCancel?.();
  };

  return (
    <form
      onSubmit={handleSubmit}
      className="flex h-full flex-col"
    >
      <div className="flex flex-1 flex-col gap-4 overflow-y-auto">
        {submitted && errors.size > 0 && (
          <div
            role="alert"
            className="rounded-md border border-danger-border bg-danger-surface px-3 py-2 text-2xs text-danger-foreground"
          >
            {t("submission.invalid", { count: errors.size })}
          </div>
        )}
        <StructuredForm
          schema={schema}
          value={record}
          onChange={setRecord}
          errors={errors}
          disabled={pending}
        />
      </div>

      <SaveBar
        dirty={dirty}
        pending={pending}
        onSave={handleSave}
        onDiscard={handleDiscard}
      />
    </form>
  );
}
