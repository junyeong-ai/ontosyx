"use client";

import { useCallback, useState } from "react";
import { useTranslations } from "next-intl";
import { z } from "zod";

import { toast } from "@/components/ui/toast";
import { Heading } from "@/components/ui/heading";
import { FormInput } from "@/components/ui/form-input";
import { createWorkspace } from "@/lib/api/workspaces";
import {
  setWorkspaceId,
  setWorkspaceName,
  setWorkspaceRole,
} from "@/lib/workspace";
import { useFormWithSchema } from "@/hooks/use-form-with-schema";

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
 * Validation schema for the create-workspace form. The schema is the
 * single source of truth — `useFormWithSchema` runs it on submit and
 * the returned error map drops onto the relevant field. Any future
 * server-side rule (slug uniqueness, name length, reserved words)
 * lands here and surfaces in the same gate.
 */
const SCHEMA = z.object({
  name: z.string().trim().min(1, { message: "errors.nameRequired" }),
  slug: z
    .string()
    .trim()
    .min(1, { message: "errors.slugRequired" })
    .regex(/^[a-z0-9][a-z0-9-_]*$/, { message: "errors.slugFormat" }),
});

type WorkspaceFormInput = z.input<typeof SCHEMA>;

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
  const tCommon = useTranslations("common");
  const [name, setName] = useState("");
  /**
   * `slugOverride` is empty when the user hasn't touched the slug
   * field. The effective slug is derived from `name` (via `toSlug`)
   * whenever the override is empty — eliminating the
   * auto-generation effect.
   */
  const [slugOverride, setSlugOverride] = useState("");
  const slug = slugOverride || toSlug(name);

  const onValid = useCallback(
    async ({ name: validName, slug: validSlug }: WorkspaceFormInput) => {
      try {
        const ws = await createWorkspace({ name: validName, slug: validSlug });
        toast.success(t("toast.created"));
        onClose();
        setWorkspaceId(ws.id);
        setWorkspaceName(ws.name);
        setWorkspaceRole("owner");
        window.location.reload();
      } catch (err) {
        toast.error(t("toast.createFailed"), {
          description:
            err instanceof Error ? err.message : t("toast.unknownError"),
        });
      }
    },
    [onClose, t],
  );

  const { errors, submit, clearErrors, pending } = useFormWithSchema({
    schema: SCHEMA,
    onValid,
  });

  const handleSubmit = useCallback(() => {
    void submit({ name, slug });
  }, [name, slug, submit]);

  // Translate the error keys at render time so the schema can
  // declare i18n key strings without pulling translations into the
  // schema definition itself.
  const nameError = errors.name ? t(errors.name) : undefined;
  const slugError = errors.slug ? t(errors.slug) : undefined;

  return (
    <div className="fixed inset-0 z-modal flex items-center justify-center">
      {/* Backdrop */}
      <div
        className="absolute inset-0 bg-surface-scrim-strong"
        onClick={onClose}
      />
      {/* Dialog */}
      <div className="relative w-full max-w-md rounded-lg border border-divider bg-surface-base p-6 shadow-4">
        <Heading level={2} size={4}>
          {t("createTitle")}
        </Heading>
        <p className="mt-1 text-sm text-foreground-muted">
          {t("createDescription")}
        </p>

        <div className="mt-5 space-y-4">
          {/* Name */}
          <div>
            <label className="mb-1 block text-xs font-medium text-foreground">
              {t("nameLabel")}
            </label>
            <FormInput
              value={name}
              onChange={(e) => {
                setName(e.target.value);
                clearErrors("name");
              }}
              autoFocus
              placeholder={t("namePlaceholder")}
              error={!!nameError}
              aria-describedby={nameError ? "create-workspace-name-error" : undefined}
              onKeyDown={(e) => {
                if (e.key === "Enter") handleSubmit();
                if (e.key === "Escape") onClose();
              }}
            />
            {nameError && (
              <p
                id="create-workspace-name-error"
                role="alert"
                className="mt-1 text-2xs text-danger-foreground"
              >
                {nameError}
              </p>
            )}
          </div>

          {/* Slug */}
          <div>
            <label className="mb-1 block text-xs font-medium text-foreground">
              {t("slugLabel")}
            </label>
            <FormInput
              value={slug}
              onChange={(e) => {
                setSlugOverride(e.target.value);
                clearErrors("slug");
              }}
              placeholder={t("slugPlaceholder")}
              className="font-mono"
              error={!!slugError}
              aria-describedby={
                slugError ? "create-workspace-slug-error" : "create-workspace-slug-hint"
              }
              onKeyDown={(e) => {
                if (e.key === "Enter") handleSubmit();
                if (e.key === "Escape") onClose();
              }}
            />
            {slugError ? (
              <p
                id="create-workspace-slug-error"
                role="alert"
                className="mt-1 text-2xs text-danger-foreground"
              >
                {slugError}
              </p>
            ) : (
              <p
                id="create-workspace-slug-hint"
                className="mt-1 text-2xs text-foreground-muted"
              >
                {t("slugHint")}
              </p>
            )}
          </div>
        </div>

        {/* Actions */}
        <div className="mt-6 flex justify-end gap-2">
          <button type="button"
            onClick={onClose}
            className="rounded-md px-3 py-1.5 text-sm text-foreground-muted hover:bg-surface-inset"
          >
            {tCommon("cancel")}
          </button>
          <button type="button"
            onClick={handleSubmit}
            disabled={pending}
            className="rounded-md bg-brand-solid px-3 py-1.5 text-sm font-medium text-foreground-onbrand hover:bg-brand-solid disabled:opacity-50"
          >
            {pending ? tCommon("creating") : tCommon("create")}
          </button>
        </div>
      </div>
    </div>
  );
}
