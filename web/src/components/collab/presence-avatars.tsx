// PresenceAvatars — stacked initials for the active members of a
// collaboration room. Each avatar is deterministically coloured by
// `user_id` so the same person reads the same hue across sessions
// and tabs.

"use client";

import { useMemo } from "react";

import { useCollabStore, selectPresence } from "@/lib/collab";
import type { PresenceInfo } from "@/lib/collab";
import { cn } from "@/lib/cn";

interface PresenceAvatarsProps {
  projectId: string;
  /** Avatars beyond this count collapse into a `+N` chip. */
  maxVisible?: number;
  /**
   * Optional user id to suppress — typically the current viewer,
   * since the workbench already shows their identity in the header.
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

  const visible = useMemo(() => {
    const filtered = excludeUserId
      ? presence.filter((p) => p.user_id !== excludeUserId)
      : presence;
    return filtered;
  }, [presence, excludeUserId]);

  if (visible.length === 0) return null;

  const overflow = Math.max(0, visible.length - maxVisible);
  const head = visible.slice(0, maxVisible);

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
        <div
          className="flex h-7 w-7 items-center justify-center rounded-full border border-background bg-zinc-200 text-2xs font-semibold text-zinc-700 dark:bg-zinc-700 dark:text-zinc-200"
          title={visible
            .slice(maxVisible)
            .map((m) => m.user_name)
            .join(", ")}
        >
          +{overflow}
        </div>
      )}
    </div>
  );
}

function PresenceAvatar({ member }: { member: PresenceInfo }) {
  const initials = initialsFor(member.user_name);
  const color = colorFor(member.user_id);
  return (
    <div
      className="flex h-7 w-7 items-center justify-center rounded-full border border-background text-2xs font-semibold text-white"
      style={{ backgroundColor: color }}
      title={member.user_name}
    >
      {initials}
    </div>
  );
}

/** First letter of each whitespace-separated name part, max two. */
function initialsFor(name: string): string {
  const parts = name.trim().split(/\s+/).slice(0, 2);
  return parts.map((p) => p[0]?.toUpperCase() ?? "").join("");
}

/**
 * Stable colour from the user id. We pick from a fixed palette
 * tuned for legible white-on-colour at small sizes; a hash maps
 * the id into the palette so the same user reads the same hue
 * across sessions and devices.
 */
const PALETTE = [
  "#0ea5e9", // sky-500
  "#10b981", // emerald-500
  "#f59e0b", // amber-500
  "#ef4444", // red-500
  "#8b5cf6", // violet-500
  "#ec4899", // pink-500
  "#14b8a6", // teal-500
  "#f97316", // orange-500
] as const;

function colorFor(userId: string): string {
  let h = 0;
  for (let i = 0; i < userId.length; i++) {
    h = (h * 31 + userId.charCodeAt(i)) >>> 0;
  }
  return PALETTE[h % PALETTE.length];
}
