"use client";

import { useEffect, useState, useMemo } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useTranslations } from "next-intl";
import { toast } from "@/components/ui/toast";
import { ArrowDown, Bot } from "lucide-react";
import { useAuth } from "@/hooks/use-auth";
import { Button } from "@/components/ui/button";
import { SettingsSwitch, SettingsSelect, SettingsInput } from "@/components/ui/form-input";
import { useImeAwareInput } from "@/hooks/use-ime-aware-input";
import { CodeEditor } from "@/components/ui/code-editor";
import { useConfirm } from "@/components/providers/confirm-provider";
import { EmptyState } from "@/components/ui/empty-state";
import { SkeletonList } from "@/components/ui/skeleton";
import { SettingsPageShell } from "@/components/layout/settings-page-shell";
import { PageStateView } from "@/components/layout/page-state-view";
import type { PageState } from "@/components/layout/page-state";
import { cn } from "@/lib/cn";
import type { PromptTemplate } from "@/types/api";
import {
  listPromptTemplates,
  createPromptTemplate,
  updatePromptTemplate,
  deletePromptTemplate,
} from "@/lib/api";

const promptsKeys = {
  all: ["prompts"] as const,
  list: () => [...promptsKeys.all, "list"] as const,
};

export default function PromptsPage() {
  const t = useTranslations("settings.runtime.prompts");
  const tCommon = useTranslations("common");
  const { isAdmin } = useAuth();
  const qc = useQueryClient();
  const [expandedName, setExpandedName] = useState<string | null>(null);
  const [search, setSearch] = useState("");
  const searchInput = useImeAwareInput("");
  useEffect(() => {
    if (searchInput.committedValue !== search) {
      setSearch(searchInput.committedValue);
    }
  }, [searchInput.committedValue, search]);
  const [statusFilter, setStatusFilter] = useState<"" | "active" | "inactive">("");

  const query = useQuery({
    queryKey: promptsKeys.list(),
    queryFn: () => listPromptTemplates(),
  });
  const templates: PromptTemplate[] | undefined = query.data;
  const reload = () => qc.invalidateQueries({ queryKey: promptsKeys.list() });

  const grouped = useMemo(() => {
    const map = new Map<string, PromptTemplate[]>();
    for (const tmpl of templates ?? []) {
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
      <SettingsPageShell title={t("title")} subtitle={t("description")}>
        <EmptyState title={t("adminOnly")} />
      </SettingsPageShell>
    );
  }

  const pageState: PageState = query.isLoading
    ? { kind: "loading" }
    : query.isError
      ? { kind: "error", onRetry: () => void query.refetch() }
      : { kind: "data" };

  return (
    <SettingsPageShell title={t("title")} subtitle={t("description")}>
      <PageStateView
        state={pageState}
        skeleton={<SkeletonList count={4} />}
        error={{
          title: tCommon("loadError.title"),
          description: tCommon("loadError.description"),
          retryLabel: tCommon("retry"),
        }}
      >
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
          <span className="ms-auto text-sm tabular-nums text-foreground-muted">
            {t("countLabel", { count: filtered.length })}
          </span>
        </div>

        <div className="mt-5">
          {filtered.length === 0 ? (
            <div className="rounded-xl border border-dashed border-divider">
              <EmptyState
                icon={Bot}
                title={t("empty")}
                description={search ? t("emptyHint") : undefined}
              />
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
                    reload();
                  }}
                  onDelete={async (id) => {
                    await deletePromptTemplate(id);
                    reload();
                  }}
                  onNewVersion={async (vName, version, content) => {
                    await createPromptTemplate({
                      name: vName,
                      version,
                      content,
                    });
                    reload();
                  }}
                />
              ))}
            </div>
          )}
        </div>
      </PageStateView>
    </SettingsPageShell>
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
  const t = useTranslations("settings.runtime.prompts");
  const activeVersion = versions.find((v) => v.is_active) || versions[0];

  return (
    <div
      className={cn(
        "rounded-xl border transition-all duration-[var(--duration-base)] ease-[var(--ease-out)]",
        isExpanded
          ? "border-brand-border bg-surface-base shadow-1"
          : "border-divider bg-surface-base hover:border-divider",
      )}
    >
      <button
        type="button"
        onClick={onToggle}
        className="flex w-full items-center gap-3 px-4 py-3 text-start"
      >
        <span className="min-w-0 flex-1 truncate text-sm font-medium text-foreground-strong">
          {name}
        </span>
        <span className="shrink-0 text-xs tabular-nums text-foreground-muted">
          v{activeVersion.version}
        </span>
        <span className="flex shrink-0 items-center gap-1.5 text-xs text-foreground-muted">
          <span
            className={cn(
              "h-2 w-2 rounded-full",
              activeVersion.is_active ? "bg-brand-solid" : "bg-foreground-muted",
            )}
          />
          {activeVersion.is_active ? t("active") : t("inactive")}
        </span>
        <span className="shrink-0 rounded bg-surface-inset px-1.5 py-0.5 text-2xs tabular-nums text-foreground-muted">
          {t("versions", { count: versions.length })}
        </span>
        <ArrowDown className={cn(
 "h-4 w-4 shrink-0 text-foreground-muted transition-transform duration-[var(--duration-quick)] ease-[var(--ease-out)]",
 isExpanded && "rotate-180",
 )} />
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
  const t = useTranslations("settings.runtime.prompts");
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
  }, [selected.content, selected.is_active]);

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
    <div className="border-t border-divider-soft px-4 pb-4 pt-3">
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
        <span className="text-xs text-foreground-muted">
          {t("authorMeta", { user: selected.created_by })} &middot;{" "}
          {new Date(selected.created_at).toLocaleDateString()}
        </span>

        <div className="ms-auto flex items-center gap-2">
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
