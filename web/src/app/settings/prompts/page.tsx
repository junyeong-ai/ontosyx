"use client";

import { useEffect, useState, useMemo, useCallback } from "react";
import { useTranslations } from "next-intl";
import { useAuth } from "@/lib/use-auth";
import { Spinner } from "@/components/ui/spinner";
import { Button } from "@/components/ui/button";
import { SettingsSwitch, SettingsSelect, SettingsInput } from "@/components/ui/form-input";
import { useImeAwareInput } from "@/lib/use-ime-aware-input";
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
  const t = useTranslations("settings.prompts");
  const { isAdmin } = useAuth();
  const [templates, setTemplates] = useState<PromptTemplate[]>([]);
  const [loading, setLoading] = useState(true);
  const [expandedName, setExpandedName] = useState<string | null>(null);
  const [search, setSearch] = useState("");
  const searchInput = useImeAwareInput("");
  useEffect(() => {
    if (searchInput.committedValue !== search) {
      setSearch(searchInput.committedValue);
    }
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [searchInput.committedValue]);
  const [statusFilter, setStatusFilter] = useState<"" | "active" | "inactive">("");

  const reload = useCallback(async () => {
    try {
      const data = await listPromptTemplates();
      setTemplates(data);
    } catch {
      toast.error(t("toast.loadFailed"));
    } finally {
      setLoading(false);
    }
  }, [t]);

  useEffect(() => {
    reload();
  }, [reload]);

  const grouped = useMemo(() => {
    const map = new Map<string, PromptTemplate[]>();
    for (const tmpl of templates) {
      const list = map.get(tmpl.name) || [];
      list.push(tmpl);
      map.set(tmpl.name, list);
    }
    for (const [, versions] of map) {
      versions.sort((a, b) => b.version.localeCompare(a.version));
    }
    return Array.from(map.entries());
  }, [templates]);

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
      <div className="flex h-full items-center justify-center text-sm text-muted-foreground">
        {t("adminOnly")}
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
        <div>
          <h1 className="text-lg font-semibold text-zinc-900 dark:text-zinc-100">
            {t("title")}
          </h1>
          <p className="mt-1 text-sm text-muted-foreground">
            {t("description")}
          </p>
        </div>

        <div className="mt-4 flex items-center gap-3">
          <SettingsInput
            label={t("searchLabel")}
            hideLabel
            type="search"
            placeholder={t("searchPlaceholder")}
            value={searchInput.value}
            onChange={searchInput.bind.onChange}
            onCompositionStart={searchInput.bind.onCompositionStart}
            onCompositionEnd={searchInput.bind.onCompositionEnd}
            className="max-w-xs"
          />
          <SettingsSelect
            label={t("statusFilterLabel")}
            hideLabel
            value={statusFilter}
            onChange={(e) => setStatusFilter(e.target.value as "" | "active" | "inactive")}
          >
            <option value="">{t("allStatus")}</option>
            <option value="active">{t("active")}</option>
            <option value="inactive">{t("inactive")}</option>
          </SettingsSelect>
          <span className="ml-auto text-sm tabular-nums text-muted-foreground">
            {t("countLabel", { count: filtered.length })}
          </span>
        </div>

        <div className="mt-5">
          {filtered.length === 0 ? (
            <div className="rounded-xl border border-dashed border-zinc-300 px-6 py-16 text-center dark:border-zinc-700">
              <p className="text-sm text-muted-foreground">{t("empty")}</p>
              {search && (
                <p className="mt-1 text-xs text-muted-foreground">
                  {t("emptyHint")}
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
                    await reload();
                  }}
                  onDelete={async (id) => {
                    await deletePromptTemplate(id);
                    setTemplates((prev) => prev.filter((x) => x.id !== id));
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
  const t = useTranslations("settings.prompts");
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
      <button
        onClick={onToggle}
        className="flex w-full items-center gap-3 px-4 py-3 text-left"
      >
        <span className="min-w-0 flex-1 truncate text-sm font-medium text-zinc-800 dark:text-zinc-200">
          {name}
        </span>
        <span className="shrink-0 text-xs tabular-nums text-muted-foreground">
          v{activeVersion.version}
        </span>
        <span className="flex shrink-0 items-center gap-1.5 text-xs text-muted-foreground">
          <span
            className={cn(
              "h-2 w-2 rounded-full",
              activeVersion.is_active ? "bg-emerald-500" : "bg-zinc-400",
            )}
          />
          {activeVersion.is_active ? t("active") : t("inactive")}
        </span>
        <span className="shrink-0 rounded bg-zinc-100 px-1.5 py-0.5 text-[10px] tabular-nums text-zinc-500 dark:bg-zinc-800 dark:text-muted-foreground">
          {t("versions", { count: versions.length })}
        </span>
        <svg
          className={cn(
            "h-4 w-4 shrink-0 text-muted-foreground transition-transform",
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
  const t = useTranslations("settings.prompts");
  const tCommon = useTranslations("common");
  const confirm = useConfirm();
  const activeVersion = versions.find((v) => v.is_active) || versions[0];
  const [selectedId, setSelectedId] = useState(activeVersion.id);
  const selected = versions.find((v) => v.id === selectedId) || versions[0];

  const [content, setContent] = useState(selected.content);
  const [isActive, setIsActive] = useState(selected.is_active);
  const [isSaving, setIsSaving] = useState(false);

  useEffect(() => {
    setContent(selected.content);
    setIsActive(selected.is_active);
  }, [selected.id, selected.content, selected.is_active]);

  const hasChanges = content !== selected.content || isActive !== selected.is_active;

  const handleSave = async () => {
    setIsSaving(true);
    try {
      await onUpdate(selected.id, { content, is_active: isActive });
      toast.success(t("toast.updated"));
    } catch {
      toast.error(t("toast.saveFailed"));
    } finally {
      setIsSaving(false);
    }
  };

  const handleDelete = async () => {
    const ok = await confirm({
      title: t("deleteConfirmTitle", { version: selected.version }),
      description: t("deleteConfirmDescription", {
        version: selected.version,
        name,
      }),
      variant: "danger",
      confirmLabel: t("deleteConfirmLabel"),
    });
    if (!ok) return;
    try {
      await onDelete(selected.id);
      toast.success(t("toast.deleted", { version: selected.version }));
    } catch {
      toast.error(t("toast.deleteFailed"));
    }
  };

  const handleNewVersion = async () => {
    const current = parseInt(selected.version, 10);
    const newVersion = String(Number.isNaN(current) ? 1 : current + 1);
    try {
      await onNewVersion(name, newVersion, content);
      toast.success(t("toast.versionCreated", { version: newVersion }));
    } catch {
      toast.error(t("toast.versionCreateFailed"));
    }
  };

  return (
    <div className="border-t border-zinc-100 px-4 pb-4 pt-3 dark:border-zinc-800">
      <div className="flex flex-wrap items-center gap-3">
        <SettingsSelect
          label={t("selectedVersion")}
          hideLabel
          value={selectedId}
          onChange={(e) => setSelectedId(e.target.value)}
          className="w-auto"
        >
          {versions.map((v) => (
            <option key={v.id} value={v.id}>
              v{v.version}
              {v.is_active ? t("versionSuffixActive") : ""}
            </option>
          ))}
        </SettingsSelect>
        <span className="text-xs text-muted-foreground">
          {t("authorMeta", { user: selected.created_by })} &middot;{" "}
          {new Date(selected.created_at).toLocaleDateString()}
        </span>

        <div className="ml-auto flex items-center gap-2">
          <SettingsSwitch
            label={t("activeSwitch")}
            checked={isActive}
            onChange={setIsActive}
          />
          <Button variant="outline" size="xs" onClick={handleNewVersion}>
            {t("newVersion")}
          </Button>
          <Button variant="danger" size="xs" onClick={handleDelete}>
            {tCommon("delete")}
          </Button>
          <Button
            variant="primary"
            size="sm"
            onClick={handleSave}
            disabled={!hasChanges || isSaving}
          >
            {isSaving ? tCommon("saving") : tCommon("save")}
          </Button>
        </div>
      </div>

      <div className="mt-3">
        <CodeEditor value={content} onChange={setContent} height="400px" />
      </div>
    </div>
  );
}
