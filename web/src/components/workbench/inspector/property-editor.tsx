"use client";

import { useState } from "react";
import { useTranslations } from "next-intl";
import { useAppStore } from "@/lib/store";
import { defaultText } from "@/lib/locale/localize";
import { cn } from "@/lib/cn";
import { HugeiconsIcon } from "@hugeicons/react";
import { Delete01Icon } from "@hugeicons/core-free-icons";
import { toast } from "@/components/ui/toast";
import { Tooltip } from "@/components/ui/tooltip";
import { Button } from "@/components/ui/button";
import { FormInput, FormSelect } from "@/components/ui/form-input";
import { Checkbox } from "@/components/ui/checkbox";
import type { PropertyDef, PropertyPatch, OntologyCommand, DataClassification } from "@/types/api";
import { formatPropertyType } from "@/types/api";
import { InlineEdit } from "./inline-edit";
import { LinkTermDropdown } from "./link-term-dropdown";
import type { OwnerKind } from "@/lib/api/binding-suggestions";

const classificationStyles: Record<
  DataClassification,
  { bg: string; text: string }
> = {
  public: {
    bg: "bg-brand-surface-strong/40",
    text: "text-brand-foreground",
  },
  internal: {
    bg: "bg-info-surface",
    text: "text-info-foreground",
  },
  confidential: {
    bg: "bg-warning-surface/40",
    text: "text-warning-foreground",
  },
  restricted: {
    bg: "bg-danger-surface/40",
    text: "text-danger-foreground",
  },
};

function ClassificationBadge({ classification }: { classification: DataClassification }) {
  const t = useTranslations("inspector.classification");
  const style = classificationStyles[classification];
  const label = t(classification);
  return (
    <Tooltip content={t("tooltip", { label })}>
      <span
        className={cn(
          "inline-flex items-center rounded px-1 py-0.5 text-2xs font-medium leading-none",
          style.bg,
          style.text,
        )}
      >
        {label}
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
  const t = useTranslations("inspector.property.addForm");
  const tCommon = useTranslations("common");
  const applyCommand = useAppStore((s) => s.applyCommand);
  const [name, setName] = useState("");
  const [propType, setPropType] = useState("string");
  const [nullable, setNullable] = useState(true);

  const handleSave = () => {
    const trimmed = name.trim();
    if (!trimmed) return;
    const cmd: OntologyCommand = {
      op: "add_property",
      owner_id: ownerId,
      property: {
        id: crypto.randomUUID(),
        name: trimmed,
        property_type: { type: propType },
        nullable,
        description: { default: "" },
      },
    };
    applyCommand(cmd);
    toast.success(t("addedToast", { name: trimmed }));
    onClose();
  };

  return (
    <div className="space-y-1.5 border-b border-dashed border-brand-border bg-brand-surface px-3 py-2">
      <FormInput
        autoFocus
        density="compact"
        placeholder={t("namePlaceholder")}
        value={name}
        onChange={(e) => setName(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter") handleSave();
          if (e.key === "Escape") onClose();
        }}
      />
      <div className="flex items-center gap-2">
        {/* i18n-audit-ignore(15) — option labels mirror wire-protocol PropertyType discriminants. */}
        <FormSelect
          density="compact"
          value={propType}
          onChange={(e) => setPropType(e.target.value)}
          className="w-auto"
        >
          <option value="string">string</option>
          <option value="int">int</option>
          <option value="float">float</option>
          <option value="bool">bool</option>
          <option value="date">date</option>
          <option value="datetime">datetime</option>
          <option value="duration">duration</option>
          <option value="bytes">bytes</option>
        </FormSelect>
        <Checkbox
          checked={nullable}
          onChange={(e) => setNullable(e.target.checked)}
          label={<span className="text-foreground-muted">{t("nullable")}</span>}
        />
      </div>
      <div className="flex gap-1.5">
        <Button
          variant="primary"
          size="xs"
          onClick={handleSave}
          disabled={!name.trim()}
        >
          {tCommon("create")}
        </Button>
        <Button variant="ghost" size="xs" onClick={onClose}>
          {tCommon("cancel")}
        </Button>
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
  const t = useTranslations("inspector.property");
  const tAria = useTranslations("inspector.aria");
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
            <FormSelect
              autoFocus
              density="compact"
              value={prop.property_type.type}
              onChange={(e) => {
                onUpdate({ property_type: { type: e.target.value } });
                setEditingType(false);
              }}
              onBlur={() => setEditingType(false)}
              className="w-auto"
            >
              {/* i18n-audit-ignore(10) — wire-protocol PropertyType discriminants. */}
              <option value="string">string</option>
              <option value="int">int</option>
              <option value="float">float</option>
              <option value="bool">bool</option>
              <option value="date">date</option>
              <option value="datetime">datetime</option>
              <option value="duration">duration</option>
              <option value="bytes">bytes</option>
            </FormSelect>
          ) : (
            <button type="button"
              onClick={() => setEditingType(true)}
              className="text-foreground-muted hover:text-foreground"
              title={t("changeTypeTooltip")}
            >
              {formatPropertyType(prop.property_type)}
            </button>
          )}
          <Tooltip content={prop.nullable ? t("nullableTooltip") : t("requiredTooltip")}>
            <button type="button"
              onClick={() => onUpdate({ nullable: !prop.nullable })}
              aria-label={prop.nullable ? t("nullableTooltip") : t("requiredTooltip")}
              className={prop.nullable ? "text-foreground-muted hover:text-warning-foreground" : "text-warning-foreground hover:text-foreground-muted"}
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
          placeholder={t("descriptionPlaceholder")}
          onSave={(description) =>
            onUpdate({ description: { default: description } })
          }
          className="mt-0.5 break-words text-foreground-muted"
        />
        {prop.source_column && (
          <p className="text-foreground-muted">{t("sourceColumn", { column: prop.source_column })}</p>
        )}
      </div>
      <Tooltip content={tAria("deleteProperty")}>
        <button type="button"
          onClick={onDelete}
          aria-label={tAria("deleteProperty")}
          className="mt-0.5 rounded p-0.5 text-foreground-muted opacity-0 transition-opacity duration-[var(--duration-quick)] ease-[var(--ease-out)] hover:text-danger-foreground group-hover:opacity-100 group-focus-within:opacity-100"
        >
          <HugeiconsIcon icon={Delete01Icon} className="h-2.5 w-2.5" size="100%" />
        </button>
      </Tooltip>
    </div>
  );
}
