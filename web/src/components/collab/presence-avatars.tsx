// PresenceAvatars — stacked initials for the active members of a
// collaboration room. Each avatar reads its hue from `colorFor`
// so it matches the user's remote cursor and lock-ring colour
// across every collaboration surface.

"use client";

import { useMemo } from "react";

import { colorFor, useCollabStore, selectPresence } from "@/lib/collab";
import type { PresenceInfo } from "@/lib/collab";
import { Tooltip } from "@/components/ui/tooltip";
import { cn } from "@/lib/cn";

interface PresenceAvatarsProps {
  projectId: string;
  /** Avatars beyond this count collapse into a `+N` chip. */
  maxVisible?: number;
  /**
   * User id to suppress — typically the current viewer, since the
   * workbench already shows their identity in the header. Pass
   * `undefined` to render every member including self.
   */
  excludeUserId?: string;
  className?: string;
}

export function PresenceAvatars({
  projectId,
  maxVisible = 4,
  excludeUserId,
  className,
}: PresenceAvatarsProps) {
  const presence = useCollabStore(selectPresence(projectId));

  const visible = useMemo(
    () =>
      excludeUserId
        ? presence.filter((p) => p.user_id !== excludeUserId)
        : presence,
    [presence, excludeUserId],
  );

  if (visible.length === 0) return null;

  const overflow = Math.max(0, visible.length - maxVisible);
  const head = visible.slice(0, maxVisible);
  const overflowNames = visible
    .slice(maxVisible)
    .map((m) => m.user_name)
    .join(", ");

  return (
    <div
      className={cn("flex items-center -space-x-1.5", className)}
      role="group"
      aria-label={`${visible.length} active collaborators`}
    >
      {head.map((member) => (
        <PresenceAvatar key={member.user_id} member={member} />
      ))}
      {overflow > 0 && (
        <Tooltip content={overflowNames}>
          <div className="flex h-7 w-7 items-center justify-center rounded-full border border-background bg-zinc-200 text-2xs font-semibold text-zinc-700 dark:bg-zinc-700 dark:text-zinc-200">
            +{overflow}
          </div>
        </Tooltip>
      )}
    </div>
  );
}

function PresenceAvatar({ member }: { member: PresenceInfo }) {
  const initials = initialsFor(member.user_name);
  const color = colorFor(member.user_id);
  return (
    <Tooltip content={member.user_name}>
      <div
        className="flex h-7 w-7 items-center justify-center rounded-full border border-background text-2xs font-semibold text-white"
        style={{ backgroundColor: color }}
      >
        {initials}
      </div>
    </Tooltip>
  );
}

/** First letter of each whitespace-separated name part, max two. */
function initialsFor(name: string): string {
  const parts = name.trim().split(/\s+/).slice(0, 2);
  return parts.map((p) => p[0]?.toUpperCase() ?? "").join("");
}
