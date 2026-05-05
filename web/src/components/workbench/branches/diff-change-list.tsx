"use client";

import { useTranslations } from "next-intl";

import type {
  EdgeChange,
  NodeChange,
  PropertyChange,
} from "@/types/ontology-branches";

/**
 * Render a single property-level change line. PropertyChange
 * variants are scalar deltas — type, nullability, default, or
 * description. Description renders the localized default; the
 * full translation map is opaque at this layer.
 */
function renderPropertyChange(
  change: PropertyChange,
  t: ReturnType<typeof useTranslations>,
): string {
  switch (change.type) {
    case "type_changed":
      return t("change.propertyType", { old: change.old, new: change.new });
    case "nullability_changed":
      return t("change.nullability", {
        old: change.old ? "nullable" : "required",
        new: change.new ? "nullable" : "required",
      });
    case "description_changed":
      return t("change.description", {
        old: change.old?.default || "",
        new: change.new?.default || "",
      });
    case "default_value_changed":
      return t("change.defaultValue", {
        old: change.old ?? "—",
        new: change.new ?? "—",
      });
  }
}

/** One node-level change line. Property-modified expands into
 *  nested PropertyChange lines so the operator sees every atom
 *  the BE emitted. */
export function NodeChangeList({ changes }: { changes: NodeChange[] }) {
  const t = useTranslations("workbench.branches.diffModal");
  return (
    <ul className="space-y-1 text-xs">
      {changes.map((c, idx) => {
        switch (c.type) {
          case "label_changed":
            return (
              <li key={idx}>
                {t("change.label", { old: c.old, new: c.new })}
              </li>
            );
          case "description_changed":
            return (
              <li key={idx}>
                {t("change.description", {
                  old: c.old?.default || "",
                  new: c.new?.default || "",
                })}
              </li>
            );
          case "property_added":
            return (
              <li key={idx} className="text-success-foreground">
                {t("change.propertyAdded", { name: c.property.name })}
              </li>
            );
          case "property_removed":
            return (
              <li key={idx} className="text-danger-foreground">
                {t("change.propertyRemoved", { name: c.property.name })}
              </li>
            );
          case "property_modified":
            return (
              <li key={idx}>
                <span>
                  {t("change.propertyModified", { name: c.property_name })}
                </span>
                {c.changes.length > 0 && (
                  <ul className="mt-1 ms-4 list-disc space-y-0.5 text-foreground-muted">
                    {c.changes.map((sub, subIdx) => (
                      <li key={subIdx}>{renderPropertyChange(sub, t)}</li>
                    ))}
                  </ul>
                )}
              </li>
            );
          case "constraint_added":
            return (
              <li key={idx} className="text-success-foreground">
                {t("change.constraintAdded", { constraint: c.constraint })}
              </li>
            );
          case "constraint_removed":
            return (
              <li key={idx} className="text-danger-foreground">
                {t("change.constraintRemoved", { constraint: c.constraint })}
              </li>
            );
          default:
            // Exhaustive switch — `c` is `never` here. Returning
            // `null` keeps biome's `useIterableCallbackReturn`
            // happy without weakening the typed contract.
            return null;
        }
      })}
    </ul>
  );
}

/** One edge-level change line. Reuses the property sub-list when
 *  the variant is property-modified. */
export function EdgeChangeList({ changes }: { changes: EdgeChange[] }) {
  const t = useTranslations("workbench.branches.diffModal");
  return (
    <ul className="space-y-1 text-xs">
      {changes.map((c, idx) => {
        switch (c.type) {
          case "label_changed":
            return (
              <li key={idx}>
                {t("change.label", { old: c.old, new: c.new })}
              </li>
            );
          case "description_changed":
            return (
              <li key={idx}>
                {t("change.description", {
                  old: c.old?.default || "",
                  new: c.new?.default || "",
                })}
              </li>
            );
          case "source_changed":
            return (
              <li key={idx}>
                {t("change.source", { old: c.old, new: c.new })}
              </li>
            );
          case "target_changed":
            return (
              <li key={idx}>
                {t("change.target", { old: c.old, new: c.new })}
              </li>
            );
          case "cardinality_changed":
            return (
              <li key={idx}>
                {t("change.cardinality", { old: c.old, new: c.new })}
              </li>
            );
          case "property_added":
            return (
              <li key={idx} className="text-success-foreground">
                {t("change.propertyAdded", { name: c.property.name })}
              </li>
            );
          case "property_removed":
            return (
              <li key={idx} className="text-danger-foreground">
                {t("change.propertyRemoved", { name: c.property.name })}
              </li>
            );
          case "property_modified":
            return (
              <li key={idx}>
                <span>
                  {t("change.propertyModified", { name: c.property_name })}
                </span>
                {c.changes.length > 0 && (
                  <ul className="mt-1 ms-4 list-disc space-y-0.5 text-foreground-muted">
                    {c.changes.map((sub, subIdx) => (
                      <li key={subIdx}>{renderPropertyChange(sub, t)}</li>
                    ))}
                  </ul>
                )}
              </li>
            );
          default:
            return null;
        }
      })}
    </ul>
  );
}
