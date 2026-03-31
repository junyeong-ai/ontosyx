"use client";

import { useState, useEffect, useCallback } from "react";
import { toast } from "sonner";
import { getConfig, updateConfig } from "@/lib/api";
import { useAuth } from "@/lib/use-auth";
import type { ConfigResponse, ConfigEntry, ConfigUpdateItem } from "@/types/api";
import { FormInput } from "@/components/ui/form-input";
import { Spinner } from "@/components/ui/spinner";

// ---------------------------------------------------------------------------
// Category display metadata
// ---------------------------------------------------------------------------

const CATEGORY_META: Record<string, { label: string; description: string }> = {
  llm: {
    label: "LLM Parameters",
    description: "Language model token limits and temperature settings",
  },
  thresholds: {
    label: "Schema Thresholds",
    description: "Size thresholds for adaptive processing",
  },
  profiling: {
    label: "Profiling",
    description: "Graph and schema profiling parameters",
  },
  timeouts: {
    label: "Timeouts",
    description: "Operation timeout durations (seconds)",
  },
  ui: {
    label: "UI / Layout",
    description: "ELK graph layout and frontend parameters",
  },
  lifecycle: {
    label: "Lifecycle",
    description: "WIP project archival and cleanup settings",
  },
};

const CATEGORY_ORDER = ["ui", "llm", "thresholds", "profiling", "timeouts", "lifecycle"];

export default function SystemSettingsPage() {
  const { isAdmin } = useAuth();
  const [config, setConfig] = useState<ConfigResponse | null>(null);
  const [editedValues, setEditedValues] = useState<Record<string, string>>({});
  const [loading, setLoading] = useState(true);
  const [isSaving, setIsSaving] = useState(false);
  const [activeTab, setActiveTab] = useState<string>(CATEGORY_ORDER[0]);

  const loadConfig = useCallback(async () => {
    setLoading(true);
    try {
      const data = await getConfig();
      setConfig(data);
      setEditedValues({});
    } catch (err) {
      toast.error("Failed to load configuration", {
        description: err instanceof Error ? err.message : "Unknown error",
      });
    } finally {
      setLoading(false);
    }
  }, []);

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
        const { [ck]: _, ...rest } = prev;
        return rest;
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
          toast.error(`Invalid value for ${key}`, {
            description: "Must be a non-negative integer",
          });
          return;
        }
      } else if (entry.data_type === "float") {
        const parsed = Number(value);
        if (isNaN(parsed)) {
          toast.error(`Invalid value for ${key}`, {
            description: "Must be a valid number",
          });
          return;
        }
      }

      updates.push({ category, key, value });
    }

    setIsSaving(true);
    try {
      await updateConfig({ updates });
      toast.success(
        `Updated ${updates.length} setting${updates.length > 1 ? "s" : ""}`,
      );
      await loadConfig();
    } catch (err) {
      toast.error("Failed to save configuration", {
        description: err instanceof Error ? err.message : "Unknown error",
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

  // Count edits per category for badge display
  const editCountByCategory = (category: string): number =>
    Object.keys(editedValues).filter((ck) => ck.startsWith(`${category}.`))
      .length;

  return (
    <div>
      <h1 className="text-xl font-semibold text-zinc-900 dark:text-zinc-100">
        System Settings
      </h1>
      <p className="mt-1 text-sm text-zinc-500 dark:text-zinc-400">
        Runtime-tunable configuration stored in the database.
      </p>

      {loading ? (
        <div className="flex items-center justify-center py-20">
          <Spinner size="lg" className="text-emerald-500" />
        </div>
      ) : (
        <>
          {/* Category tabs */}
          <div className="mt-6 flex gap-1 border-b border-zinc-200 dark:border-zinc-800">
            {categories.map((category) => {
              const meta = CATEGORY_META[category];
              const editCount = editCountByCategory(category);
              return (
                <button
                  key={category}
                  onClick={() => setActiveTab(category)}
                  className={`relative px-4 py-2 text-sm font-medium transition-colors ${
                    activeTab === category
                      ? "text-emerald-700 dark:text-emerald-400"
                      : "text-zinc-500 hover:text-zinc-700 dark:text-zinc-400 dark:hover:text-zinc-300"
                  }`}
                >
                  {meta?.label ?? category}
                  {editCount > 0 && (
                    <span className="ml-1.5 inline-flex h-4 min-w-4 items-center justify-center rounded-full bg-amber-100 px-1 text-[10px] font-bold text-amber-700 dark:bg-amber-900/40 dark:text-amber-400">
                      {editCount}
                    </span>
                  )}
                  {activeTab === category && (
                    <span className="absolute inset-x-0 -bottom-px h-0.5 bg-emerald-500" />
                  )}
                </button>
              );
            })}
          </div>

          {/* Active category content */}
          <div className="mt-4">
            {categories
              .filter((c) => c === activeTab)
              .map((category) => (
                <ConfigCategory
                  key={category}
                  category={category}
                  entries={config![category]}
                  meta={CATEGORY_META[category]}
                  getCurrentValue={(entry) =>
                    getCurrentValue(category, entry)
                  }
                  onChange={(key, value) =>
                    handleChange(category, key, value)
                  }
                />
              ))}
          </div>

          {/* Sticky footer */}
          <div className="sticky bottom-0 mt-8 flex items-center justify-end gap-2 border-t border-zinc-200 bg-zinc-50 px-0 py-4 dark:border-zinc-800 dark:bg-zinc-950">
            {hasChanges && (
              <span className="mr-auto text-xs text-amber-600 dark:text-amber-400">
                {Object.keys(editedValues).length} unsaved{" "}
                {Object.keys(editedValues).length === 1
                  ? "change"
                  : "changes"}
              </span>
            )}
            {hasChanges && (
              <button
                onClick={handleReset}
                className="rounded-lg px-4 py-2 text-sm font-medium text-zinc-600 transition-colors hover:bg-zinc-200 dark:text-zinc-400 dark:hover:bg-zinc-800"
              >
                Discard
              </button>
            )}
            <button
              onClick={handleSave}
              disabled={!isAdmin || !hasChanges || isSaving}
              className="rounded-lg bg-emerald-600 px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-emerald-700 disabled:cursor-not-allowed disabled:opacity-50"
            >
              {isSaving ? "Saving..." : "Save Changes"}
            </button>
          </div>
        </>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Category Section
// ---------------------------------------------------------------------------

interface ConfigCategoryProps {
  category: string;
  entries: ConfigEntry[];
  meta?: { label: string; description: string };
  getCurrentValue: (entry: ConfigEntry) => string;
  onChange: (key: string, value: string) => void;
}

function ConfigCategory({
  category,
  entries,
  meta,
  getCurrentValue,
  onChange,
}: ConfigCategoryProps) {
  return (
    <section className="rounded-lg border border-zinc-200 bg-white dark:border-zinc-800 dark:bg-zinc-900">
      <div className="border-b border-zinc-100 px-6 py-4 dark:border-zinc-800">
        <h2 className="text-sm font-semibold text-zinc-900 dark:text-zinc-100">
          {meta?.label ?? category}
        </h2>
        {meta?.description && (
          <p className="mt-0.5 text-xs text-zinc-500 dark:text-zinc-400">
            {meta.description}
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

// ---------------------------------------------------------------------------
// Individual Config Field
// ---------------------------------------------------------------------------

interface ConfigFieldProps {
  entry: ConfigEntry;
  value: string;
  onChange: (value: string) => void;
}

function ConfigField({ entry, value, onChange }: ConfigFieldProps) {
  const isModified = value !== entry.value;

  return (
    <div className="flex items-center gap-3 px-6 py-3">
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2">
          <span className="text-sm font-medium text-zinc-700 dark:text-zinc-300">
            {formatKeyLabel(entry.key)}
          </span>
          <span className="rounded bg-zinc-200/60 px-1.5 py-0.5 text-[10px] font-medium text-zinc-500 dark:bg-zinc-700 dark:text-zinc-400">
            {entry.data_type}
          </span>
          {isModified && (
            <span className="rounded bg-amber-100 px-1.5 py-0.5 text-[10px] font-medium text-amber-700 dark:bg-amber-900/30 dark:text-amber-400">
              modified
            </span>
          )}
        </div>
        <p className="mt-0.5 text-xs text-zinc-500 dark:text-zinc-400">
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

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function formatKeyLabel(key: string): string {
  return key
    .split("_")
    .map((w) => w.charAt(0).toUpperCase() + w.slice(1))
    .join(" ");
}
