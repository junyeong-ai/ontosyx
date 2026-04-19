"use client";

import { useState } from "react";
import { useTranslations } from "next-intl";
import { toast } from "sonner";
import { createWorkspace } from "@/lib/api/workspaces";
import { setWorkspaceId, setWorkspaceName, setWorkspaceRole } from "@/lib/workspace";

interface Props {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

function toSlug(name: string): string {
  return name
    .toLowerCase()
    .replace(/[^a-z0-9-_]/g, "-")
    .replace(/-+/g, "-")
    .replace(/^-|-$/g, "");
}

/**
 * Why two components: the outer is a pass-through that mounts the body
 * only when `open`. Remounting resets all body state on every open —
 * replacing the two `useEffect + setState` resets that were flagged by
 * React 19's `set-state-in-effect` gate. `return null` on the body
 * itself would keep it alive and we'd need the reset effect again.
 */
export function CreateWorkspaceDialog({ open, onOpenChange }: Props) {
  if (!open) return null;
  return <CreateWorkspaceDialogBody onClose={() => onOpenChange(false)} />;
}

function CreateWorkspaceDialogBody({ onClose }: { onClose: () => void }) {
  const t = useTranslations("workspaceDialog");
  const [name, setName] = useState("");
  /**
   * `slugOverride` is empty when the user hasn't touched the slug field.
   * The effective slug is derived from `name` (via `toSlug`) whenever
   * the override is empty — eliminating the auto-generation effect.
   */
  const [slugOverride, setSlugOverride] = useState("");
  const [submitting, setSubmitting] = useState(false);

  const slug = slugOverride || toSlug(name);

  const handleSubmit = async () => {
    const trimmed = name.trim();
    const finalSlug = slug.trim() || toSlug(trimmed);
    if (!trimmed || !finalSlug) return;

    setSubmitting(true);
    try {
      const ws = await createWorkspace({ name: trimmed, slug: finalSlug });
      toast.success(t("createdToast"));
      onClose();
      setWorkspaceId(ws.id);
      setWorkspaceName(ws.name);
      setWorkspaceRole("owner");
      window.location.reload();
    } catch (err) {
      toast.error(t("createFailed"), {
        description: err instanceof Error ? err.message : t("unknownError"),
      });
      setSubmitting(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      {/* Backdrop */}
      <div
        className="absolute inset-0 bg-black/40 dark:bg-black/60"
        onClick={onClose}
      />
      {/* Dialog */}
      <div className="relative w-full max-w-md rounded-lg border border-zinc-200 bg-white p-6 shadow-xl dark:border-zinc-700 dark:bg-zinc-900">
        <h2 className="text-lg font-semibold text-zinc-900 dark:text-zinc-100">
          {t("createTitle")}
        </h2>
        <p className="mt-1 text-sm text-zinc-500 dark:text-muted-foreground">
          {t("createDescription")}
        </p>

        <div className="mt-5 space-y-4">
          {/* Name */}
          <div>
            <label className="mb-1 block text-xs font-medium text-zinc-700 dark:text-zinc-300">
              {t("nameLabel")}
            </label>
            <input
              value={name}
              onChange={(e) => setName(e.target.value)}
              autoFocus
              placeholder={t("namePlaceholder")}
              className="w-full rounded-md border border-zinc-200 bg-white px-3 py-2 text-sm text-zinc-700 focus:border-emerald-400 focus:ring-1 focus:ring-emerald-400/50 focus:outline-none dark:border-zinc-700 dark:bg-zinc-800 dark:text-zinc-300"
              onKeyDown={(e) => {
                if (e.key === "Enter") handleSubmit();
                if (e.key === "Escape") onClose();
              }}
            />
          </div>

          {/* Slug */}
          <div>
            <label className="mb-1 block text-xs font-medium text-zinc-700 dark:text-zinc-300">
              {t("slugLabel")}
            </label>
            <input
              value={slug}
              onChange={(e) => setSlugOverride(e.target.value)}
              placeholder={t("slugPlaceholder")}
              className="w-full rounded-md border border-zinc-200 bg-white px-3 py-2 text-sm font-mono text-zinc-700 focus:border-emerald-400 focus:ring-1 focus:ring-emerald-400/50 focus:outline-none dark:border-zinc-700 dark:bg-zinc-800 dark:text-zinc-300"
              onKeyDown={(e) => {
                if (e.key === "Enter") handleSubmit();
                if (e.key === "Escape") onClose();
              }}
            />
            <p className="mt-1 text-[11px] text-muted-foreground">
              {t("slugHint")}
            </p>
          </div>
        </div>

        {/* Actions */}
        <div className="mt-6 flex justify-end gap-2">
          <button
            onClick={onClose}
            className="rounded-md px-3 py-1.5 text-sm text-muted-foreground hover:bg-zinc-100 dark:hover:bg-zinc-800"
          >
            {t("cancel")}
          </button>
          <button
            onClick={handleSubmit}
            disabled={!name.trim() || !slug.trim() || submitting}
            className="rounded-md bg-emerald-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-emerald-700 disabled:opacity-50"
          >
            {submitting ? t("creating") : t("create")}
          </button>
        </div>
      </div>
    </div>
  );
}
