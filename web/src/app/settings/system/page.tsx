"use client";

import { useState, useEffect, useCallback } from "react";
import { toast } from "sonner";
import { useTranslations } from "next-intl";
import { getConfig, updateConfig } from "@/lib/api";
import { useAuth } from "@/hooks/use-auth";
import type { ConfigResponse, ConfigEntry, ConfigUpdateItem } from "@/types/api";
import { FormInput } from "@/components/ui/form-input";
import { SettingsPageShell } from "@/components/layout/settings-page-shell";
import { Spinner } from "@/components/ui/spinner";
import { TabBar } from "@/components/ui/tab-bar";
import { ErrorState } from "@/components/ui/error-state";

const CATEGORY_ORDER = ["ui", "llm", "thresholds", "profiling", "timeouts", "lifecycle"] as const;
type KnownCategory = (typeof CATEGORY_ORDER)[number];
function isKnownCategory(c: string): c is KnownCategory {
  return (CATEGORY_ORDER as readonly string[]).includes(c);
}

export default function SystemSettingsPage() {
  const t = useTranslations("settings.system");
  const tCommon = useTranslations("common");
  const { isAdmin } = useAuth();
  const [config, setConfig] = useState<ConfigResponse | null>(null);
  const [editedValues, setEditedValues] = useState<Record<string, string>>({});
  const [loading, setLoading] = useState(true);
  const [isSaving, setIsSaving] = useState(false);
  const [activeTab, setActiveTab] = useState<string>(CATEGORY_ORDER[0]);

  const categoryLabel = (c: string) =>
    isKnownCategory(c) ? t(`category.${c}.label`) : c;
  const categoryDescription = (c: string) =>
    isKnownCategory(c) ? t(`category.${c}.description`) : undefined;

  const loadConfig = useCallback(async () => {
    setLoading(true);
    try {
      const data = await getConfig();
      setConfig(data);
      setEditedValues({});
    } catch (err) {
      toast.error(t("toast.loadFailed"), {
        description: err instanceof Error ? err.message : t("toast.unknownError"),
      });
    } finally {
      setLoading(false);
    }
  }, [t]);

  useEffect(() => {
    loadConfig();
  }, [loadConfig]);

  const compositeKey = (category: string, key: string) =>
    `${category}.${key}`;

  const handleChange = (category: string, key: string, value: string) => {
    const ck = compositeKey(category, key);
    const original = config?.[category]?.find((e) => e.key === key)?.value;

    setEditedValues((prev) => {
      if (value === original) {
        const next = { ...prev };
        delete next[ck];
        return next;
      }
      return { ...prev, [ck]: value };
    });
  };

  const getCurrentValue = (category: string, entry: ConfigEntry): string => {
    const ck = compositeKey(category, entry.key);
    return ck in editedValues ? editedValues[ck] : entry.value;
  };

  const hasChanges = Object.keys(editedValues).length > 0;

  const handleSave = async () => {
    if (!config || !hasChanges) return;

    const updates: ConfigUpdateItem[] = [];
    for (const [ck, value] of Object.entries(editedValues)) {
      const [category, key] = ck.split(".", 2);
      const entry = config[category]?.find((e) => e.key === key);
      if (!entry) continue;

      if (entry.data_type === "int") {
        const parsed = Number(value);
        if (!Number.isInteger(parsed) || parsed < 0) {
          toast.error(t("toast.invalidValue", { key }), {
            description: t("toast.mustBeNonNegativeInt"),
          });
          return;
        }
      } else if (entry.data_type === "float") {
        const parsed = Number(value);
        if (isNaN(parsed)) {
          toast.error(t("toast.invalidValue", { key }), {
            description: t("toast.mustBeNumber"),
          });
          return;
        }
      }

      updates.push({ category, key, value });
    }

    setIsSaving(true);
    try {
      await updateConfig({ updates });
      toast.success(t("toast.updated", { count: updates.length }));
      await loadConfig();
    } catch (err) {
      toast.error(t("toast.saveFailed"), {
        description: err instanceof Error ? err.message : t("toast.unknownError"),
      });
    } finally {
      setIsSaving(false);
    }
  };

  const handleReset = () => {
    setEditedValues({});
  };

  const editCountByCategory = (category: string): number =>
    Object.keys(editedValues).filter((ck) => ck.startsWith(`${category}.`))
      .length;

  if (loading) {
    return (
      <SettingsPageShell title={t("title")} subtitle={t("description")}>
        <div className="flex items-center justify-center py-20">
          <Spinner size="lg" className="text-brand-foreground" />
        </div>
      </SettingsPageShell>
    );
  }

  if (!config) {
    return (
      <SettingsPageShell title={t("title")} subtitle={t("description")}>
        <div className="py-12">
          <ErrorState
            title={tCommon("loadError.title")}
            description={tCommon("loadError.description")}
            onRetry={loadConfig}
            retryLabel={tCommon("retry")}
          />
        </div>
      </SettingsPageShell>
    );
  }

  const categories = CATEGORY_ORDER.filter((c) => c in config);

  return (
    <SettingsPageShell title={t("title")} subtitle={t("description")}>
      <div className="mt-6 border-b border-divider">
        <TabBar
          tabs={categories.map((category) => ({
            id: category,
            label: categoryLabel(category),
            badge: editCountByCategory(category),
          }))}
          activeTab={activeTab}
          onTabChange={setActiveTab}
        />
      </div>

      <div className="mt-4">
        {categories
          .filter((c) => c === activeTab)
          .map((category) => (
            <ConfigCategory
              key={category}
              label={categoryLabel(category)}
              description={categoryDescription(category)}
              entries={config[category]}
              getCurrentValue={(entry) =>
                getCurrentValue(category, entry)
              }
              onChange={(key, value) =>
                handleChange(category, key, value)
              }
            />
          ))}
      </div>

      <div className="sticky bottom-0 mt-8 flex items-center justify-end gap-2 border-t border-divider bg-surface-raised px-0 py-4">
        {hasChanges && (
          <span className="mr-auto text-xs text-warning-foreground">
            {t("unsavedChanges", { count: Object.keys(editedValues).length })}
          </span>
        )}
        {hasChanges && (
          <button
            onClick={handleReset}
            className="rounded-lg px-4 py-2 text-sm font-medium text-foreground transition-colors hover:bg-surface-inset"
          >
            {t("discard")}
          </button>
        )}
        <button
          onClick={handleSave}
          disabled={!isAdmin || !hasChanges || isSaving}
          className="rounded-lg bg-brand-solid px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-brand-solid-hover disabled:cursor-not-allowed disabled:opacity-50"
        >
          {isSaving ? tCommon("saving") : tCommon("save")}
        </button>
      </div>
    </SettingsPageShell>
  );
}

interface ConfigCategoryProps {
  label: string;
  description?: string;
  entries: ConfigEntry[];
  getCurrentValue: (entry: ConfigEntry) => string;
  onChange: (key: string, value: string) => void;
}

function ConfigCategory({
  label,
  description,
  entries,
  getCurrentValue,
  onChange,
}: ConfigCategoryProps) {
  return (
    <section className="rounded-lg border border-divider bg-surface-base">
      <div className="border-b border-divider-soft px-6 py-4">
        <h2 className="text-sm font-semibold text-foreground-strong">
          {label}
        </h2>
        {description && (
          <p className="mt-0.5 text-xs text-foreground-muted">
            {description}
          </p>
        )}
      </div>
      <div className="divide-y divide-divider-soft">
        {entries.map((entry) => (
          <ConfigField
            key={entry.key}
            entry={entry}
            value={getCurrentValue(entry)}
            onChange={(v) => onChange(entry.key, v)}
          />
        ))}
      </div>
    </section>
  );
}

interface ConfigFieldProps {
  entry: ConfigEntry;
  value: string;
  onChange: (value: string) => void;
}

function ConfigField({ entry, value, onChange }: ConfigFieldProps) {
  const t = useTranslations("settings.system");
  const isModified = value !== entry.value;

  return (
    <div className="flex items-center gap-3 px-6 py-3">
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2">
          <span className="text-sm font-medium text-foreground">
            {formatKeyLabel(entry.key)}
          </span>
          <span className="rounded bg-surface-inset px-1.5 py-0.5 text-2xs font-medium text-foreground-muted dark:text-muted-foreground">
            {entry.data_type}
          </span>
          {isModified && (
            <span className="rounded bg-warning-surface px-1.5 py-0.5 text-2xs font-medium text-warning-foreground">
              {t("modified")}
            </span>
          )}
        </div>
        <p className="mt-0.5 text-xs text-foreground-muted">
          {entry.description}
        </p>
      </div>
      <div className="w-36 shrink-0">
        <FormInput
          inputMode={
            entry.data_type === "int"
              ? "numeric"
              : entry.data_type === "float"
                ? "decimal"
                : "text"
          }
          value={value}
          onChange={(e) => onChange(e.target.value)}
          className={`text-right text-sm ${isModified ? "border-warning-border" : ""}`}
        />
      </div>
    </div>
  );
}

// Config key is snake_case from the backend; `max_tokens` → `Max Tokens`.
function formatKeyLabel(key: string): string {
  return key
    .split("_")
    .map((w) => w.charAt(0).toUpperCase() + w.slice(1))
    .join(" ");
}
