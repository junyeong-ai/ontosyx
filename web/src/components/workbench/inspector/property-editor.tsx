"use client";

import { useState } from "react";
import { useAppStore } from "@/lib/store";
import { defaultText } from "@/lib/locale/localize";
import { HugeiconsIcon } from "@hugeicons/react";
import { Delete01Icon } from "@hugeicons/core-free-icons";
import { toast } from "sonner";
import { Tooltip } from "@/components/ui/tooltip";
import type { PropertyDef, PropertyPatch, OntologyCommand, DataClassification } from "@/types/api";
import { formatPropertyType } from "@/types/api";
import { InlineEdit } from "./inline-edit";
import { LinkTermDropdown } from "./link-term-dropdown";
import type { OwnerKind } from "@/lib/api/binding-suggestions";

// ---------------------------------------------------------------------------
// Classification badge
// ---------------------------------------------------------------------------

const classificationStyles: Record<
  DataClassification,
  { bg: string; text: string; label: string }
> = {
  public: {
    bg: "bg-brand-surface-strong-strong/40",
    text: "text-brand-foreground",
    label: "Public",
  },
  internal: {
    bg: "bg-info-surface dark:bg-info-foreground/40",
    text: "text-info-foreground dark:text-info-foreground",
    label: "Internal",
  },
  confidential: {
    bg: "bg-warning-surface/40",
    text: "text-warning-foreground",
    label: "Confidential",
  },
  restricted: {
    bg: "bg-danger-surface/40",
    text: "text-danger-foreground",
    label: "Restricted",
  },
};

function ClassificationBadge({ classification }: { classification: DataClassification }) {
  const style = classificationStyles[classification];
  return (
    <Tooltip content={`Data classification: ${style.label}`}>
      <span
        className={`inline-flex items-center rounded px-1 py-0.5 text-2xs font-medium leading-none ${style.bg} ${style.text}`}
      >
        {style.label}
      </span>
    </Tooltip>
  );
}

// ---------------------------------------------------------------------------
// Add property form
// ---------------------------------------------------------------------------

export function PropertyEditor({
  ownerId,
  onClose,
}: {
  ownerId: string;
  onClose: () => void;
}) {
  const applyCommand = useAppStore((s) => s.applyCommand);
  const [name, setName] = useState("");
  const [propType, setPropType] = useState("string");
  const [nullable, setNullable] = useState(true);

  const handleSave = () => {
    if (!name.trim()) return;
    const cmd: OntologyCommand = {
      op: "add_property",
      owner_id: ownerId,
      property: {
        id: crypto.randomUUID(),
        name: name.trim(),
        property_type: { type: propType },
        nullable,
        description: { default: "" },
      },
    };
    applyCommand(cmd);
    toast.success(`Property "${name.trim()}" added`);
    onClose();
  };

  return (
    <div className="space-y-1.5 border-b border-dashed border-brand-border bg-brand-surface px-3 py-2">
      <input
        autoFocus
        placeholder="Property name"
        value={name}
        onChange={(e) => setName(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter") handleSave();
          if (e.key === "Escape") onClose();
        }}
        className="w-full rounded border border-divider bg-surface-base px-2 py-1 text-xs outline-none focus:border-brand-border dark:border-divider"
      />
      <div className="flex items-center gap-2">
        <select
          value={propType}
          onChange={(e) => setPropType(e.target.value)}
          className="rounded border border-divider bg-surface-base px-1.5 py-0.5 text-xs dark:border-divider"
        >
          <option value="string">string</option>
          <option value="int">int</option>
          <option value="float">float</option>
          <option value="bool">bool</option>
          <option value="date">date</option>
          <option value="datetime">datetime</option>
          <option value="duration">duration</option>
          <option value="bytes">bytes</option>
        </select>
        <label className="flex items-center gap-1 text-2xs text-muted-foreground">
          <input
            type="checkbox"
            checked={nullable}
            onChange={(e) => setNullable(e.target.checked)}
          />
          Nullable
        </label>
      </div>
      <div className="flex gap-1.5">
        <button
          onClick={handleSave}
          disabled={!name.trim()}
          className="rounded bg-brand-solid px-2.5 py-1 text-2xs font-medium text-white hover:bg-brand-solid disabled:opacity-50"
        >
          Add
        </button>
        <button
          onClick={onClose}
          className="rounded px-2.5 py-1 text-2xs text-muted-foreground hover:bg-surface-inset dark:hover:bg-surface-base"
        >
          Cancel
        </button>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Property row (editable)
// ---------------------------------------------------------------------------

export interface PropertyRowBindingContext {
  ontologyId: string;
  expectedVersion: number;
  ownerKind: OwnerKind;
  ownerTypeId: string;
}

export function PropertyRow({
  prop,
  onDelete,
  onUpdate,
  binding,
}: {
  prop: PropertyDef;
  onDelete: () => void;
  onUpdate: (patch: PropertyPatch) => void;
  /**
   * When provided, the row renders the "Link term" affordance.
   * Absent for surfaces where the edit log is not wired.
   */
  binding?: PropertyRowBindingContext;
}) {
  const [editingType, setEditingType] = useState(false);

  return (
    <div className="group flex items-start gap-1.5 border-b border-divider-soft px-3 py-1.5">
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-1.5">
          <InlineEdit
            value={prop.name}
            onSave={(name) => onUpdate({ name })}
            className="font-medium text-foreground"
          />
          {editingType ? (
            <select
              autoFocus
              value={prop.property_type.type}
              onChange={(e) => {
                onUpdate({ property_type: { type: e.target.value } });
                setEditingType(false);
              }}
              onBlur={() => setEditingType(false)}
              className="rounded border border-divider bg-surface-base px-1 py-0.5 text-2xs dark:border-divider"
            >
              <option value="string">string</option>
              <option value="int">int</option>
              <option value="float">float</option>
              <option value="bool">bool</option>
              <option value="date">date</option>
              <option value="datetime">datetime</option>
              <option value="duration">duration</option>
              <option value="bytes">bytes</option>
            </select>
          ) : (
            <button
              onClick={() => setEditingType(true)}
              className="text-muted-foreground hover:text-foreground hover:underline dark:hover:text-foreground-muted"
              title="Click to change type"
            >
              {formatPropertyType(prop.property_type)}
            </button>
          )}
          <Tooltip content={prop.nullable ? "Nullable — click to make required" : "Required — click to make nullable"}>
            <button
              onClick={() => onUpdate({ nullable: !prop.nullable })}
              aria-label={prop.nullable ? "Nullable — click to make required" : "Required — click to make nullable"}
              className={prop.nullable ? "text-muted-foreground hover:text-warning-foreground" : "text-warning-foreground hover:text-muted-foreground"}
            >
              {prop.nullable ? "?" : "*"}
            </button>
          </Tooltip>
          {prop.classification && (
            <ClassificationBadge classification={prop.classification} />
          )}
          {binding && (
            <LinkTermDropdown
              ontologyId={binding.ontologyId}
              expectedVersion={binding.expectedVersion}
              ownerKind={binding.ownerKind}
              ownerTypeId={binding.ownerTypeId}
              propertyId={prop.id}
              boundTermId={
                prop.bindings?.find((b) => b.kind === "glossary")?.id
                ?? undefined
              }
            />
          )}
        </div>
        <InlineEdit
          value={defaultText(prop.description)}
          placeholder="Add description..."
          onSave={(description) =>
            onUpdate({ description: { default: description } })
          }
          className="mt-0.5 break-words text-muted-foreground"
        />
        {prop.source_column && (
          <p className="text-muted-foreground">Column: {prop.source_column}</p>
        )}
      </div>
      <Tooltip content="Delete property">
        <button
          onClick={onDelete}
          aria-label="Delete property"
          className="mt-0.5 rounded p-0.5 text-foreground-muted opacity-0 transition-opacity hover:text-danger-foreground group-hover:opacity-100 group-focus-within:opacity-100"
        >
          <HugeiconsIcon icon={Delete01Icon} className="h-2.5 w-2.5" size="100%" />
        </button>
      </Tooltip>
    </div>
  );
}
