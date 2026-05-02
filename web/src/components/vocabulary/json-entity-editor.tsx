"use client";

import { useState, type FormEvent } from "react";

import { Button } from "@/components/ui/button";
import { SettingsTextarea } from "@/components/ui/form-input";

export interface JsonEntityEditorLabels {
  /** Heading shown above the textarea. */
  title: string;
  /** Textarea field label. */
  jsonLabel: string;
  /** Submit button label for create mode. */
  submitCreate: string;
  /** Submit button label for edit mode. */
  submitUpdate: string;
  /** Cancel button label. */
  cancel: string;
  /** Error: textarea is empty on submit. */
  errorEmpty: string;
  /** Error: JSON parse failed. Renders with `{message}` placeholder. */
  errorInvalidJsonTemplate: (message: string) => string;
}

interface JsonEntityEditorProps<T> {
  /** Initial document when editing; `undefined` produces an empty
   *  textarea with the schema's hint as placeholder. */
  initial?: T;
  /** Placeholder schema hint — rendered as the empty-state in the
   *  textarea so the operator sees the expected JSON shape. */
  schemaHint: string;
  /** All user-facing copy. Caller threads in `useTranslations`-resolved
   *  strings — keeps the editor namespace-agnostic so the same
   *  component serves rules / mappings / vocabulary surfaces. */
  labels: JsonEntityEditorLabels;
  onSubmit: (def: T) => void;
  onCancel: () => void;
  pending?: boolean;
}

/**
 * JSON-first editor — the dbt pattern adapted for any IR entity.
 * Operators paste / hand-edit the canonical wire shape; the editor
 * parses on submit and rejects malformed JSON before hitting the
 * API.
 *
 * Per-kind form helpers can land alongside this surface
 * incrementally without locking out advanced edits — JSON mode is
 * always available as the power-user fallback that round-trips
 * perfectly because it *is* the wire shape.
 */
export function JsonEntityEditor<T>({
  initial,
  schemaHint,
  labels,
  onSubmit,
  onCancel,
  pending = false,
}: JsonEntityEditorProps<T>) {
  // Initial values are seeded once. Callers that need to bind the
  // editor to a different entity remount it via a `key` prop (per
  // React 19's `react-hooks/set-state-in-effect` recommendation)
  // rather than relying on a state-resetting effect.
  const [text, setText] = useState(() =>
    initial ? JSON.stringify(initial, null, 2) : "",
  );
  const [error, setError] = useState<string | null>(null);

  const handleSubmit = (e: FormEvent) => {
    e.preventDefault();
    if (!text.trim()) {
      setError(labels.errorEmpty);
      return;
    }
    try {
      const parsed = JSON.parse(text) as T;
      onSubmit(parsed);
    } catch (parseErr) {
      setError(labels.errorInvalidJsonTemplate((parseErr as Error).message));
    }
  };

  return (
    <form onSubmit={handleSubmit} className="flex flex-col gap-3">
      <h3 className="text-sm font-semibold text-foreground-strong">
        {labels.title}
      </h3>
      <SettingsTextarea
        label={labels.jsonLabel}
        value={text}
        onChange={(e) => {
          setText(e.target.value);
          if (error) setError(null);
        }}
        rows={20}
        placeholder={schemaHint}
        className="font-mono text-[11px]"
      />
      {error && (
        <p className="rounded border border-danger-border bg-danger-surface px-2 py-1 text-xs text-danger-foreground dark:border-danger-border/50 dark:text-danger-foreground">
          {error}
        </p>
      )}
      <div className="flex items-center justify-end gap-2">
        <Button
          type="button"
          variant="ghost"
          size="sm"
          onClick={onCancel}
          disabled={pending}
        >
          {labels.cancel}
        </Button>
        <Button type="submit" size="sm" disabled={pending || !text.trim()}>
          {initial ? labels.submitUpdate : labels.submitCreate}
        </Button>
      </div>
    </form>
  );
}
