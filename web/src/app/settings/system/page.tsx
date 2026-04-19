"use client";

import { useState, useEffect, useCallback } from "react";
import { toast } from "sonner";
import { useTranslations } from "next-intl";
import { getConfig, updateConfig } from "@/lib/api";
import { useAuth } from "@/lib/use-auth";
import type { ConfigResponse, ConfigEntry, ConfigUpdateItem } from "@/types/api";
import { FormInput } from "@/components/ui/form-input";
import { Spinner } from "@/components/ui/spinner";
import { TabBar } from "@/components/ui/tab-bar";

const CATEGORY_ORDER = ["ui", "llm", "thresholds", "profiling", "timeouts", "lifecycle"] as const;
type KnownCategory = (typeof CATEGORY_ORDER)[number];
function isKnownCategory(c: string): c is KnownCategory {
  return (CATEGORY_ORDER as readonly string[]).includes(c);
}

export default function SystemSettingsPage() {
  const t = useTranslations("settings.system");
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
      toast.error(t("loadError"), {
        description: err instanceof Error ? err.message : t("unknownError"),
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
          toast.error(t("invalidValue", { key }), {
            description: t("mustBeNonNegativeInt"),
          });
          return;
        }
      } else if (entry.data_type === "float") {
        const parsed = Number(value);
        if (isNaN(parsed)) {
          toast.error(t("invalidValue", { key }), {
            description: t("mustBeNumber"),
          });
          return;
        }
      }

      updates.push({ category, key, value });
    }

    setIsSaving(true);
    try {
      await updateConfig({ updates });
      toast.success(t("updatedToast", { count: updates.length }));
      await loadConfig();
    } catch (err) {
      toast.error(t("saveError"), {
        description: err instanceof Error ? err.message : t("unknownError"),
      });
    } finally {
      setIsSaving(false);
    }
  };

  const handleReset = () => {
    setEditedValues({});
  };

  const categories = config
    ? CATEGORY_ORDER.filter((c) => c in config)
    : [];

  const editCountByCategory = (category: string): number =>
    Object.keys(editedValues).filter((ck) => ck.startsWith(`${category}.`))
      .length;

  return (
    <div>
      <h1 className="text-xl font-semibold text-zinc-900 dark:text-zinc-100">
        {t("title")}
      </h1>
      <p className="mt-1 text-sm text-zinc-500 dark:text-muted-foreground">
        {t("description")}
      </p>

      {loading ? (
        <div className="flex items-center justify-center py-20">
          <Spinner size="lg" className="text-emerald-500" />
        </div>
      ) : (
        <>
          <div className="mt-6 border-b border-zinc-200 dark:border-zinc-800">
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
                  entries={config![category]}
                  getCurrentValue={(entry) =>
                    getCurrentValue(category, entry)
                  }
                  onChange={(key, value) =>
                    handleChange(category, key, value)
                  }
                />
              ))}
          </div>

          <div className="sticky bottom-0 mt-8 flex items-center justify-end gap-2 border-t border-zinc-200 bg-zinc-50 px-0 py-4 dark:border-zinc-800 dark:bg-zinc-950">
            {hasChanges && (
              <span className="mr-auto text-xs text-amber-600 dark:text-amber-400">
                {t("unsavedChanges", { count: Object.keys(editedValues).length })}
              </span>
            )}
            {hasChanges && (
              <button
                onClick={handleReset}
                className="rounded-lg px-4 py-2 text-sm font-medium text-zinc-600 transition-colors hover:bg-zinc-200 dark:text-muted-foreground dark:hover:bg-zinc-800"
              >
                {t("discard")}
              </button>
            )}
            <button
              onClick={handleSave}
              disabled={!isAdmin || !hasChanges || isSaving}
              className="rounded-lg bg-emerald-600 px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-emerald-700 disabled:cursor-not-allowed disabled:opacity-50"
            >
              {isSaving ? t("saving") : t("save")}
            </button>
          </div>
        </>
      )}
    </div>
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
    <section className="rounded-lg border border-zinc-200 bg-white dark:border-zinc-800 dark:bg-zinc-900">
      <div className="border-b border-zinc-100 px-6 py-4 dark:border-zinc-800">
        <h2 className="text-sm font-semibold text-zinc-900 dark:text-zinc-100">
          {label}
        </h2>
        {description && (
          <p className="mt-0.5 text-xs text-zinc-500 dark:text-muted-foreground">
            {description}
          </p>
        )}
      </div>
      <div className="divide-y divide-zinc-100 dark:divide-zinc-800">
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
          <span className="text-sm font-medium text-zinc-700 dark:text-zinc-300">
            {formatKeyLabel(entry.key)}
          </span>
          <span className="rounded bg-zinc-200/60 px-1.5 py-0.5 text-[10px] font-medium text-zinc-500 dark:bg-zinc-700 dark:text-muted-foreground">
            {entry.data_type}
          </span>
          {isModified && (
            <span className="rounded bg-amber-100 px-1.5 py-0.5 text-[10px] font-medium text-amber-700 dark:bg-amber-900/30 dark:text-amber-400">
              {t("modified")}
            </span>
          )}
        </div>
        <p className="mt-0.5 text-xs text-zinc-500 dark:text-muted-foreground">
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
          className={`text-right text-sm ${isModified ? "border-amber-400 dark:border-amber-600" : ""}`}
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
