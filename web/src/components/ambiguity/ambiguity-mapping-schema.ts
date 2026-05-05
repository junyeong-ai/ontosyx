// Discriminated-union schema for the ambiguity-resolution form.
//
// Three variants — `value_map`, `code_system_ref`, `glossary_ref` —
// match the Rust `AmbiguityMapping` wire enum 1:1. Each variant
// validates its own required fields; the schema's discriminator is
// `kind` so an invalid value-map entry never produces "code_system_id
// missing" noise from a sibling variant.
//
// Adding a new mapping kind is a single variant + a `case` in
// `toAmbiguityMapping()`; the resolution modal's render branch
// follows.

import { z } from "zod";

import type { AmbiguityMapping } from "@/lib/api/ambiguity";

const ValueMapEntryFormSchema = z.object({
  value: z.string(),
  display: z.string(),
  definition: z.string(),
});

const ValueMapVariant = z.object({
  kind: z.literal("value_map"),
  entries: z
    .array(ValueMapEntryFormSchema)
    .refine(
      (rows) =>
        rows.some(
          (e) => e.value.trim() !== "" && e.display.trim() !== "",
        ),
      { message: "errors.valueMapEmpty" },
    ),
});

const CodeSystemRefVariant = z.object({
  kind: z.literal("code_system_ref"),
  code_system_id: z
    .string()
    .trim()
    .min(1, { message: "errors.codeSystemRequired" }),
});

const GlossaryRefVariant = z.object({
  kind: z.literal("glossary_ref"),
  term_id: z.string().trim().min(1, { message: "errors.termRequired" }),
});

export const AmbiguityMappingFormSchema = z.discriminatedUnion("kind", [
  ValueMapVariant,
  CodeSystemRefVariant,
  GlossaryRefVariant,
]);

export type AmbiguityMappingFormInput = z.input<
  typeof AmbiguityMappingFormSchema
>;
export type ValidatedAmbiguityMapping = z.infer<
  typeof AmbiguityMappingFormSchema
>;

/**
 * Translate the validated form value into the canonical wire shape.
 * For `value_map`, filters out incomplete rows (either value or
 * display blank) and elides empty definitions to `undefined` so the
 * server-side default kicks in.
 */
export function toAmbiguityMapping(
  v: ValidatedAmbiguityMapping,
): AmbiguityMapping {
  switch (v.kind) {
    case "value_map":
      return {
        kind: "value_map",
        entries: v.entries
          .filter((e) => e.value.trim() !== "" && e.display.trim() !== "")
          .map((e) => ({
            value: e.value,
            display: e.display,
            definition: e.definition.trim() ? e.definition : undefined,
          })),
      };
    case "code_system_ref":
      return { kind: "code_system_ref", code_system_id: v.code_system_id };
    case "glossary_ref":
      return { kind: "glossary_ref", term_id: v.term_id };
  }
}
