"use client";

import { useEffect, useState, useMemo, useCallback } from "react";
import { useAuth } from "@/lib/use-auth";
import { Spinner } from "@/components/ui/spinner";
import { Button } from "@/components/ui/button";
import { SettingsSwitch, SettingsSelect, SettingsInput } from "@/components/ui/form-input";
import { CodeEditor } from "@/components/ui/code-editor";
import { useConfirm } from "@/components/ui/confirm-dialog";
import { toast } from "sonner";
import { cn } from "@/lib/cn";
import type { PromptTemplate } from "@/types/api";
import {
  listPromptTemplates,
  createPromptTemplate,
  updatePromptTemplate,
  deletePromptTemplate,
} from "@/lib/api";

export default function PromptsPage() {
  const { isAdmin } = useAuth();
  const [templates, setTemplates] = useState<PromptTemplate[]>([]);
  const [loading, setLoading] = useState(true);
  const [expandedName, setExpandedName] = useState<string | null>(null);
  const [search, setSearch] = useState("");
  const [statusFilter, setStatusFilter] = useState<"" | "active" | "inactive">("");

  const reload = useCallback(async () => {
    try {
      const data = await listPromptTemplates();
      setTemplates(data);
    } catch {
      toast.error("Failed to load prompts");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    reload();
  }, [reload]);

  // Group templates by name, each group has versions sorted DESC
  const grouped = useMemo(() => {
    const map = new Map<string, PromptTemplate[]>();
    for (const t of templates) {
      const list = map.get(t.name) || [];
      list.push(t);
      map.set(t.name, list);
    }
    // Sort versions within each group (newest first)
    for (const [, versions] of map) {
      versions.sort((a, b) => b.version.localeCompare(a.version));
    }
    return Array.from(map.entries());
  }, [templates]);

  // Apply search and status filter
  const filtered = useMemo(() => {
    return grouped.filter(([name, versions]) => {
      if (search && !name.toLowerCase().includes(search.toLowerCase())) return false;
      if (statusFilter === "active" && !versions.some((v) => v.is_active)) return false;
      if (statusFilter === "inactive" && versions.some((v) => v.is_active)) return false;
      return true;
    });
  }, [grouped, search, statusFilter]);

  if (!isAdmin) {
    return (
      <div className="flex h-full items-center justify-center text-sm text-zinc-500">
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
    <div className="h-full overflow-y-auto">
      <div className="mx-auto">
        {/* Header */}
        <div>
          <h1 className="text-lg font-semibold text-zinc-900 dark:text-zinc-100">
            Prompt Templates
          </h1>
          <p className="mt-1 text-sm text-zinc-500">
            Manage versioned prompt templates. Changes take effect on next agent session.
          </p>
        </div>

        {/* Search + Filter */}
        <div className="mt-4 flex items-center gap-3">
          <SettingsInput
            placeholder="Search prompts..."
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            className="max-w-xs"
          />
          <SettingsSelect
            value={statusFilter}
            onChange={(e) => setStatusFilter(e.target.value as "" | "active" | "inactive")}
          >
            <option value="">All status</option>
            <option value="active">Active</option>
            <option value="inactive">Inactive</option>
          </SettingsSelect>
          <span className="ml-auto text-sm tabular-nums text-zinc-400">
            {filtered.length} prompt{filtered.length !== 1 ? "s" : ""}
          </span>
        </div>

        {/* Cards */}
        <div className="mt-5">
          {filtered.length === 0 ? (
            <div className="rounded-xl border border-dashed border-zinc-300 px-6 py-16 text-center dark:border-zinc-700">
              <p className="text-sm text-zinc-500">No prompt templates found.</p>
              {search && (
                <p className="mt-1 text-xs text-zinc-400">
                  Try adjusting your search or filter.
                </p>
              )}
            </div>
          ) : (
            <div className="space-y-2">
              {filtered.map(([name, versions]) => (
                <PromptCard
                  key={name}
                  name={name}
                  versions={versions}
                  isExpanded={expandedName === name}
                  onToggle={() =>
                    setExpandedName(expandedName === name ? null : name)
                  }
                  onUpdate={async (id, req) => {
                    await updatePromptTemplate(id, req);
                    // Reload all templates to reflect auto-deactivation
                    await reload();
                  }}
                  onDelete={async (id) => {
                    await deletePromptTemplate(id);
                    setTemplates((prev) => prev.filter((t) => t.id !== id));
                  }}
                  onNewVersion={async (vName, version, content) => {
                    const created = await createPromptTemplate({
                      name: vName,
                      version,
                      content,
                    });
                    setTemplates((prev) => [created, ...prev]);
                  }}
                />
              ))}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// PromptCard — one expandable card per prompt name
// ---------------------------------------------------------------------------

function PromptCard({
  name,
  versions,
  isExpanded,
  onToggle,
  onUpdate,
  onDelete,
  onNewVersion,
}: {
  name: string;
  versions: PromptTemplate[];
  isExpanded: boolean;
  onToggle: () => void;
  onUpdate: (id: string, req: { content?: string; is_active?: boolean }) => Promise<void>;
  onDelete: (id: string) => Promise<void>;
  onNewVersion: (name: string, version: string, content: string) => Promise<void>;
}) {
  const activeVersion = versions.find((v) => v.is_active) || versions[0];

  return (
    <div
      className={cn(
        "rounded-xl border transition-all",
        isExpanded
          ? "border-emerald-200 bg-white shadow-sm dark:border-emerald-800/40 dark:bg-zinc-900"
          : "border-zinc-200 bg-white hover:border-zinc-300 dark:border-zinc-800 dark:bg-zinc-900 dark:hover:border-zinc-700",
      )}
    >
      {/* Collapsed header */}
      <button
        onClick={onToggle}
        className="flex w-full items-center gap-3 px-4 py-3 text-left"
      >
        <span className="min-w-0 flex-1 truncate text-sm font-medium text-zinc-800 dark:text-zinc-200">
          {name}
        </span>
        <span className="shrink-0 text-xs tabular-nums text-zinc-400">
          v{activeVersion.version}
        </span>
        <span className="flex shrink-0 items-center gap-1.5 text-xs text-zinc-500">
          <span
            className={cn(
              "h-2 w-2 rounded-full",
              activeVersion.is_active
                ? "bg-emerald-500"
                : "bg-zinc-400",
            )}
          />
          {activeVersion.is_active ? "Active" : "Inactive"}
        </span>
        <span className="shrink-0 rounded bg-zinc-100 px-1.5 py-0.5 text-[10px] tabular-nums text-zinc-500 dark:bg-zinc-800 dark:text-zinc-400">
          {versions.length} version{versions.length !== 1 ? "s" : ""}
        </span>
        <svg
          className={cn(
            "h-4 w-4 shrink-0 text-zinc-400 transition-transform",
            isExpanded && "rotate-180",
          )}
          fill="none"
          viewBox="0 0 24 24"
          stroke="currentColor"
          strokeWidth={2}
        >
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            d="M19 9l-7 7-7-7"
          />
        </svg>
      </button>

      {/* Expanded detail */}
      {isExpanded && (
        <PromptCardDetail
          name={name}
          versions={versions}
          onUpdate={onUpdate}
          onDelete={onDelete}
          onNewVersion={onNewVersion}
        />
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// PromptCardDetail — expanded content with version selector + editor
// ---------------------------------------------------------------------------

function PromptCardDetail({
  name,
  versions,
  onUpdate,
  onDelete,
  onNewVersion,
}: {
  name: string;
  versions: PromptTemplate[];
  onUpdate: (id: string, req: { content?: string; is_active?: boolean }) => Promise<void>;
  onDelete: (id: string) => Promise<void>;
  onNewVersion: (name: string, version: string, content: string) => Promise<void>;
}) {
  const confirm = useConfirm();
  const activeVersion = versions.find((v) => v.is_active) || versions[0];
  const [selectedId, setSelectedId] = useState(activeVersion.id);
  const selected = versions.find((v) => v.id === selectedId) || versions[0];

  const [content, setContent] = useState(selected.content);
  const [isActive, setIsActive] = useState(selected.is_active);
  const [isSaving, setIsSaving] = useState(false);

  // Sync editor state when switching versions
  useEffect(() => {
    setContent(selected.content);
    setIsActive(selected.is_active);
  }, [selected.id, selected.content, selected.is_active]);

  const hasChanges = content !== selected.content || isActive !== selected.is_active;

  const handleSave = async () => {
    setIsSaving(true);
    try {
      await onUpdate(selected.id, { content, is_active: isActive });
      toast.success("Prompt updated");
    } catch {
      toast.error("Save failed");
    } finally {
      setIsSaving(false);
    }
  };

  const handleDelete = async () => {
    const ok = await confirm({
      title: `Delete version v${selected.version}?`,
      description: `This will permanently delete version v${selected.version} of "${name}". This action cannot be undone.`,
      variant: "danger",
      confirmLabel: "Delete",
    });
    if (!ok) return;
    try {
      await onDelete(selected.id);
      toast.success(`Version v${selected.version} deleted`);
    } catch {
      toast.error("Delete failed");
    }
  };

  const handleNewVersion = async () => {
    const current = parseInt(selected.version, 10);
    const newVersion = String(Number.isNaN(current) ? 1 : current + 1);
    try {
      await onNewVersion(name, newVersion, content);
      toast.success(`Version v${newVersion} created`);
    } catch {
      toast.error("Failed to create version");
    }
  };

  return (
    <div className="border-t border-zinc-100 px-4 pb-4 pt-3 dark:border-zinc-800">
      {/* Version selector + metadata + actions */}
      <div className="flex flex-wrap items-center gap-3">
        <SettingsSelect
          value={selectedId}
          onChange={(e) => setSelectedId(e.target.value)}
          className="w-auto"
        >
          {versions.map((v) => (
            <option key={v.id} value={v.id}>
              v{v.version}
              {v.is_active ? " (active)" : ""}
            </option>
          ))}
        </SettingsSelect>
        <span className="text-xs text-zinc-400">
          by {selected.created_by} &middot;{" "}
          {new Date(selected.created_at).toLocaleDateString()}
        </span>

        <div className="ml-auto flex items-center gap-2">
          <SettingsSwitch label="Active" checked={isActive} onChange={setIsActive} />
          <Button variant="outline" size="xs" onClick={handleNewVersion}>
            New Version
          </Button>
          <Button variant="danger" size="xs" onClick={handleDelete}>
            Delete
          </Button>
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

      {/* Code editor */}
      <div className="mt-3">
        <CodeEditor value={content} onChange={setContent} height="400px" />
      </div>
    </div>
  );
}
