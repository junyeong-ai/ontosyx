"use client";

// LockIndicator — visual cue that an entity is currently held by
// someone (you or another collaborator). Sits in the top-right
// corner of an entity card / inspector header and tints to the
// holder's identity colour, the same hue used for that user's
// avatar and remote cursor.

import { useTranslations } from "next-intl";

import { colorFor, selectPresence, useCollabStore } from "@/lib/collab";
import { Tooltip } from "@/components/ui/tooltip";
import { useEntityLock } from "./use-entity-lock";
import { cn } from "@/lib/cn";

interface LockIndicatorProps {
  projectId: string | undefined;
  entityId: string | undefined;
  className?: string;
}

export function LockIndicator({
  projectId,
  entityId,
  className,
}: LockIndicatorProps) {
  const lock = useEntityLock(projectId, entityId);
  const presence = useCollabStore(selectPresence(projectId ?? ""));
  const t = useTranslations("collaboration.lock");

  if (lock.kind === "unlocked") return null;

  if (lock.kind === "locked-by-me") {
    return (
      <Tooltip content={t("editingByYou")}>
        <span
          className={cn(
            "inline-flex h-5 w-5 items-center justify-center rounded-full",
            "bg-success-surface text-success-foreground",
            className,
          )}
          aria-label={t("editingByYou")}
        >
          <PencilIcon />
        </span>
      </Tooltip>
    );
  }

  // locked-by-other
  const holderName =
    presence.find((p) => p.user_id === lock.heldBy)?.user_name ?? lock.heldBy;
  const holderColor = colorFor(lock.heldBy);

  return (
    <Tooltip content={t("editingBy", { name: holderName })}>
      <span
        className={cn(
          "inline-flex h-5 w-5 items-center justify-center rounded-full text-white",
          className,
        )}
        style={{ backgroundColor: holderColor }}
        aria-label={t("editingBy", { name: holderName })}
      >
        <LockIcon />
      </span>
    </Tooltip>
  );
}

function PencilIcon() {
  return (
    <svg width="11" height="11" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
      <path d="M11.5 1.5a1.5 1.5 0 0 1 2.121 0l.879.879a1.5 1.5 0 0 1 0 2.121L5.5 13.5l-3.5 1 1-3.5L11.5 1.5z" />
    </svg>
  );
}

function LockIcon() {
  return (
    <svg width="11" height="11" viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
      <path d="M5 7V5a3 3 0 0 1 6 0v2h.5A1.5 1.5 0 0 1 13 8.5v5A1.5 1.5 0 0 1 11.5 15h-7A1.5 1.5 0 0 1 3 13.5v-5A1.5 1.5 0 0 1 4.5 7H5zm1.5 0h3V5a1.5 1.5 0 0 0-3 0v2z" />
    </svg>
  );
}
