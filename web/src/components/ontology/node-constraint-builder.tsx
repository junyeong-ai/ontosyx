"use client";

import { useMemo, useState } from "react";
import { useTranslations } from "next-intl";
import { HugeiconsIcon } from "@hugeicons/react";
import {
  PlusSignIcon,
  Cancel01Icon,
  Delete01Icon,
} from "@hugeicons/core-free-icons";

import type { ConstraintDef, NodeTypeDef, PropertyDef } from "@/types/ontology";
import { arr } from "@/lib/ir-collections";
import { Tooltip } from "@/components/ui/tooltip";
import { FormSelect } from "@/components/ui/form-input";

export type ConstraintKind = ConstraintDef["type"];

const KINDS: readonly ConstraintKind[] = ["unique", "exists", "node_key"];

export interface NodeConstraintBuilderProps {
  node: NodeTypeDef;
  /** Add a new constraint to this node. The caller mints the id and
   *  dispatches the right `OntologyCommand` (`add_constraint`). */
  onAdd: (constraint: ConstraintDef) => void;
  /** Remove an existing constraint by id. Caller dispatches
   *  `remove_constraint`. */
  onRemove: (constraintId: string) => void;
  readOnly?: boolean;
}

/**
 * Display + edit `NodeTypeDef.constraints` (unique / exists /
 * node_key — structural assertions bound to a node type).
 *
 * Distinct from the SHACL `ConstraintForm` in
 * `components/settings/vocabulary/` which edits rule-level
 * `ShaclConstraint` values (MinCount / Datatype / MatchesPattern,
 * etc.). The two surfaces share the word "constraint" but operate
 * on different IR collections — keep the editors separate so the
 * picker + property-list shape stays specific to its domain.
 */
export function NodeConstraintBuilder({
  node,
  onAdd,
  onRemove,
  readOnly = false,
}: NodeConstraintBuilderProps) {
  const t = useTranslations("ontology.nodeConstraintBuilder");
  const constraints = arr(node.constraints);
  const properties = arr(node.properties);

  return (
    <div className="space-y-2">
      {constraints.length === 0 ? (
        <p className="text-2xs italic text-foreground-muted">
          {t("emptyState")}
        </p>
      ) : (
        <ul className="divide-y divide-divider-soft rounded border border-divider-soft">
          {constraints.map((c) => (
            <ConstraintRow
              key={c.id}
              constraint={c}
              properties={properties}
              onRemove={readOnly ? undefined : () => onRemove(c.id)}
              t={t}
            />
          ))}
        </ul>
      )}
      {!readOnly && (
        <AddConstraintForm properties={properties} onAdd={onAdd} t={t} />
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Existing-constraint row
// ---------------------------------------------------------------------------

function ConstraintRow({
  constraint,
  properties,
  onRemove,
  t,
}: {
  constraint: ConstraintDef;
  properties: readonly PropertyDef[];
  onRemove?: () => void;
  t: (key: string, params?: Record<string, string | number>) => string;
}) {
  const summary = formatConstraint(constraint, properties, t);
  return (
    <li className="group flex items-center justify-between gap-2 px-2 py-1.5">
      <span className="text-2xs text-foreground-strong">
        {summary}
      </span>
      {onRemove && (
        <Tooltip content={t("removeAction")}>
          <button
            type="button"
            onClick={onRemove}
            aria-label={t("removeAriaLabel", {
              summary,
            })}
            className="rounded p-0.5 text-foreground-muted opacity-0 transition-opacity duration-[var(--duration-quick)] ease-[var(--ease-out)] hover:text-danger-foreground group-hover:opacity-100"
          >
            <HugeiconsIcon
              icon={Delete01Icon}
              className="h-3 w-3"
              size="100%"
            />
          </button>
        </Tooltip>
      )}
    </li>
  );
}

// ---------------------------------------------------------------------------
// Add-constraint form
// ---------------------------------------------------------------------------

function AddConstraintForm({
  properties,
  onAdd,
  t,
}: {
  properties: readonly PropertyDef[];
  onAdd: (constraint: ConstraintDef) => void;
  t: (key: string, params?: Record<string, string | number>) => string;
}) {
  const [open, setOpen] = useState(false);
  const [kind, setKind] = useState<ConstraintKind>("unique");
  const [selected, setSelected] = useState<readonly string[]>([]);

  const validSelection = useMemo(() => {
    if (kind === "exists") return selected.length === 1;
    return selected.length >= 1;
  }, [kind, selected]);

  const reset = () => {
    setOpen(false);
    setKind("unique");
    setSelected([]);
  };

  const submit = () => {
    if (!validSelection) return;
    const id = `cd-${crypto.randomUUID()}`;
    const constraint: ConstraintDef =
      kind === "exists"
        ? { id, type: "exists", property_id: selected[0] }
        : { id, type: kind, property_ids: [...selected] };
    onAdd(constraint);
    reset();
  };

  if (!open) {
    return (
      <button
        type="button"
        onClick={() => setOpen(true)}
        disabled={properties.length === 0}
        className="inline-flex items-center gap-1 rounded border border-dashed border-divider px-2 py-1 text-2xs text-foreground-muted hover:border-concept-border hover:text-concept-foreground disabled:opacity-50"
      >
        <HugeiconsIcon icon={PlusSignIcon} className="h-3 w-3" size="100%" />
        {t("addAction")}
      </button>
    );
  }

  return (
    <div className="rounded border border-concept-border bg-concept-surface p-2">
      <div className="flex flex-wrap items-center gap-2">
        <FormSelect
          value={kind}
          onChange={(e) => {
            const next = e.target.value as ConstraintKind;
            setKind(next);
            if (next === "exists") setSelected(selected.slice(0, 1));
          }}
          density="compact"
          className="w-auto"
        >
          {KINDS.map((k) => (
            <option key={k} value={k}>
              {t(`kinds.${k}.label`)}
            </option>
          ))}
        </FormSelect>
        <span className="text-2xs text-foreground-muted">
          {t(`kinds.${kind}.hint`)}
        </span>
        <button
          type="button"
          onClick={reset}
          aria-label={t("cancelAction")}
          className="ms-auto rounded p-0.5 text-foreground-muted hover:bg-surface-inset"
        >
          <HugeiconsIcon icon={Cancel01Icon} className="h-3 w-3" size="100%" />
        </button>
      </div>

      <PropertyMultiSelect
        properties={properties}
        selected={selected}
        onChange={setSelected}
        single={kind === "exists"}
        emptyLabel={t("noProperties")}
      />

      <div className="mt-2 flex justify-end">
        <button
          type="button"
          onClick={submit}
          disabled={!validSelection}
          className="rounded bg-concept-foreground px-2.5 py-1 text-2xs font-medium text-foreground-onbrand hover:bg-concept-foreground disabled:opacity-50"
        >
          {t("submitAction")}
        </button>
      </div>
    </div>
  );
}

function PropertyMultiSelect({
  properties,
  selected,
  onChange,
  single,
  emptyLabel,
}: {
  properties: readonly PropertyDef[];
  selected: readonly string[];
  onChange: (next: string[]) => void;
  single: boolean;
  emptyLabel: string;
}) {
  if (properties.length === 0) {
    return (
      <p className="mt-1 text-2xs italic text-foreground-muted">
        {emptyLabel}
      </p>
    );
  }
  const toggle = (id: string) => {
    if (single) {
      onChange([id]);
      return;
    }
    onChange(
      selected.includes(id)
        ? selected.filter((s) => s !== id)
        : [...selected, id],
    );
  };
  return (
    <ul className="mt-2 flex flex-wrap gap-1.5">
      {properties.map((prop) => {
        const isSelected = selected.includes(prop.id);
        return (
          <li key={prop.id}>
            <button
              type="button"
              onClick={() => toggle(prop.id)}
              className={
                "inline-flex items-center gap-1 rounded-full border px-2 py-0.5 text-2xs transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)] " +
                (isSelected
                  ? "border-concept-border bg-concept-surface text-concept-foreground"
                  : "border-divider bg-surface-base text-foreground hover:border-concept-border hover:text-concept-foreground-muted")
              }
            >
              {prop.name}
            </button>
          </li>
        );
      })}
    </ul>
  );
}

// ---------------------------------------------------------------------------
// formatConstraint — shared with inspector's read-only renderer
// ---------------------------------------------------------------------------

export function formatConstraint(
  c: ConstraintDef,
  properties: readonly PropertyDef[],
  t: (key: string, params?: Record<string, string | number>) => string,
): string {
  const resolveName = (pid: string) =>
    properties.find((p) => p.id === pid)?.name ?? pid;
  switch (c.type) {
    case "unique":
      return t("kinds.unique.summary", {
        properties: c.property_ids.map(resolveName).join(", "),
      });
    case "exists":
      return t("kinds.exists.summary", {
        property: resolveName(c.property_id),
      });
    case "node_key":
      return t("kinds.node_key.summary", {
        properties: c.property_ids.map(resolveName).join(", "),
      });
  }
}
