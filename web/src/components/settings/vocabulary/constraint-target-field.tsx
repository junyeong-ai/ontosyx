"use client";

import type { ChangeEvent } from "react";
import { useTranslations } from "next-intl";

import { SettingsInput } from "@/components/ui/form-input";
import type { ConstraintTarget } from "@/lib/api/edit-ops";

interface ConstraintTargetFieldProps {
  label: string;
  value: ConstraintTarget;
  onChange: (next: ConstraintTarget) => void;
}

/**
 * Editor for a [`ConstraintTarget`] union. Renders a kind picker
 * plus the kind-specific id fields. `inherit` carries no payload —
 * the picker collapses the payload row when selected.
 *
 * Used inside [`ConstraintForm`] for any field declared with
 * `kind: "constraint_target"` / `"constraint_target_pair"`.
 */
export function ConstraintTargetField({
  label,
  value,
  onChange,
}: ConstraintTargetFieldProps) {
  const t = useTranslations(
    "settings.vocabulary.rules.constraints.targetKinds",
  );

  const handleKindChange = (e: ChangeEvent<HTMLSelectElement>) => {
    const next = e.target.value as ConstraintTarget["kind"];
    switch (next) {
      case "inherit":
        onChange({ kind: "inherit" });
        break;
      case "property":
        onChange({
          kind: "property",
          node_type_id: "",
          property_id: "",
        });
        break;
      case "node_type":
        onChange({ kind: "node_type", node_type_id: "" });
        break;
      case "edge_label":
        onChange({ kind: "edge_label", edge_label: "" });
        break;
    }
  };

  return (
    <div className="flex flex-col gap-2 rounded border border-divider-soft p-2 dark:border-divider">
      <label className="text-2xs font-medium uppercase tracking-wide text-muted-foreground dark:text-muted-foreground">
        {label}
      </label>
      <select
        value={value.kind}
        onChange={handleKindChange}
        className="rounded border border-divider-soft bg-white px-2 py-1 text-xs dark:border-divider dark:bg-surface-base"
      >
        <option value="inherit">{t("inherit")}</option>
        <option value="property">{t("property")}</option>
        <option value="node_type">{t("nodeType")}</option>
        <option value="edge_label">{t("edgeLabel")}</option>
      </select>
      {value.kind === "property" && (
        <div className="grid grid-cols-2 gap-2">
          <SettingsInput
            label={t("nodeType")}
            value={value.node_type_id}
            onChange={(e) =>
              onChange({ ...value, node_type_id: e.target.value })
            }
          />
          <SettingsInput
            label={t("propertyId")}
            value={value.property_id}
            onChange={(e) =>
              onChange({ ...value, property_id: e.target.value })
            }
          />
        </div>
      )}
      {value.kind === "node_type" && (
        <SettingsInput
          label={t("nodeType")}
          value={value.node_type_id}
          onChange={(e) =>
            onChange({ ...value, node_type_id: e.target.value })
          }
        />
      )}
      {value.kind === "edge_label" && (
        <SettingsInput
          label={t("edgeLabel")}
          value={value.edge_label}
          onChange={(e) =>
            onChange({ ...value, edge_label: e.target.value })
          }
        />
      )}
    </div>
  );
}
