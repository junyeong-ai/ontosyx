"use client";

import { useEffect, useState } from "react";
import { useAuth } from "@/lib/use-auth";
import { Spinner } from "@/components/ui/spinner";
import { Button } from "@/components/ui/button";
import { StatusBadge } from "@/components/ui/status-badge";
import { SettingsSection } from "@/components/settings/settings-section";
import { SettingsListDetail } from "@/components/settings/settings-list-detail";
import { toast } from "sonner";
import type { PromptTemplate } from "@/types/api";
import { listPromptTemplates, createPromptTemplate, updatePromptTemplate } from "@/lib/api";

const PROMPT_STATUS_COLORS: Record<string, string> = {
  active: "bg-emerald-100 text-emerald-700 dark:bg-emerald-900/30 dark:text-emerald-400",
  inactive: "bg-zinc-200 text-zinc-500 dark:bg-zinc-700 dark:text-zinc-400",
};

export default function PromptsPage() {
  const { isAdmin } = useAuth();
  const [templates, setTemplates] = useState<PromptTemplate[]>([]);
  const [loading, setLoading] = useState(true);
  const [selectedId, setSelectedId] = useState<string | null>(null);

  useEffect(() => {
    listPromptTemplates()
      .then(setTemplates)
      .catch(() => toast.error("Failed to load prompts"))
      .finally(() => setLoading(false));
  }, []);

  if (!isAdmin) {
    return (
      <div className="text-sm text-zinc-500">
        Admin access required to manage prompts.
      </div>
    );
  }

  if (loading) {
    return (
      <div className="flex items-center justify-center py-12">
        <Spinner size="lg" />
      </div>
    );
  }

  return (
    <SettingsSection
      title="Prompt Templates"
      description="Manage versioned prompt templates. Changes take effect on next agent session."
    >
      <SettingsListDetail<PromptTemplate>
        title="Prompt Templates"
        items={templates}
        selectedId={selectedId}
        onSelect={setSelectedId}
        getId={(t) => t.id}
        renderListItem={(t) => (
          <>
            <div className="font-medium">{t.name}</div>
            <div className="flex items-center gap-2 text-xs text-zinc-400">
              <span>v{t.version}</span>
              <StatusBadge
                status={t.is_active ? "active" : "inactive"}
                colorMap={PROMPT_STATUS_COLORS}
                className="px-1.5 py-0 text-[9px]"
              />
            </div>
          </>
        )}
        renderDetail={(t) => (
          <PromptEditor
            template={t}
            onSave={async (content, isActive) => {
              await updatePromptTemplate(t.id, { content, is_active: isActive });
              setTemplates((prev) =>
                prev.map((item) =>
                  item.id === t.id ? { ...item, content, is_active: isActive } : item,
                ),
              );
              toast.success("Prompt updated");
            }}
            onNewVersion={async (name, version, content) => {
              const created = await createPromptTemplate({ name, version, content });
              setTemplates((prev) => [created, ...prev]);
              setSelectedId(created.id);
              toast.success(`Version ${version} created`);
            }}
          />
        )}
      />
    </SettingsSection>
  );
}

function PromptEditor({
  template,
  onSave,
  onNewVersion,
}: {
  template: PromptTemplate;
  onSave: (content: string, isActive: boolean) => Promise<void>;
  onNewVersion: (name: string, version: string, content: string) => Promise<void>;
}) {
  const [content, setContent] = useState(template.content);
  const [isActive, setIsActive] = useState(template.is_active);
  const [isSaving, setIsSaving] = useState(false);

  useEffect(() => {
    setContent(template.content);
    setIsActive(template.is_active);
  }, [template.id]);

  const handleSave = async () => {
    setIsSaving(true);
    try {
      await onSave(content, isActive);
    } catch {
      toast.error("Save failed");
    } finally {
      setIsSaving(false);
    }
  };

  const hasChanges = content !== template.content || isActive !== template.is_active;

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-sm font-semibold text-zinc-800 dark:text-zinc-200">
            {template.name}
          </h2>
          <p className="text-xs text-zinc-400">
            v{template.version} · by {template.created_by} · {new Date(template.created_at).toLocaleDateString()}
          </p>
        </div>
        <div className="flex items-center gap-3">
          <Button
            variant="outline"
            size="sm"
            onClick={async () => {
              const parts = template.version.split(".");
              const patch = parseInt(parts[2] || "0") + 1;
              const newVersion = `${parts[0]}.${parts[1]}.${patch}`;
              await onNewVersion(template.name, newVersion, content);
            }}
            className="text-[10px]"
          >
            New Version
          </Button>
          <label className="flex items-center gap-2 text-xs text-zinc-600 dark:text-zinc-400">
            <input
              type="checkbox"
              checked={isActive}
              onChange={(e) => setIsActive(e.target.checked)}
              className="rounded border-zinc-300"
            />
            Active
          </label>
          <Button
            variant="primary"
            size="sm"
            onClick={handleSave}
            disabled={!hasChanges || isSaving}
          >
            {isSaving ? "Saving..." : "Save"}
          </Button>
        </div>
      </div>

      <textarea
        value={content}
        onChange={(e) => setContent(e.target.value)}
        rows={24}
        className="w-full rounded-md border border-zinc-200 bg-zinc-50 p-3 font-mono text-xs text-zinc-800 dark:border-zinc-700 dark:bg-zinc-900 dark:text-zinc-200 focus:border-emerald-500 focus:outline-none focus:ring-2 focus:ring-emerald-500/50"
      />
    </div>
  );
}
