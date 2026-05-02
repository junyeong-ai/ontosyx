// PresenceAvatars — stacked initials for the active members of a
// collaboration room. Each avatar reads its hue from `colorFor`
// so it matches the user's remote cursor and lock-ring colour
// across every collaboration surface. Hovering an avatar opens a
// detail card sourced from the workspace member roster — name,
// email, role.

"use client";

import { useMemo } from "react";

import { colorFor, useCollabStore, selectPresence } from "@/lib/collab";
import type { PresenceInfo } from "@/lib/collab";
import { Tooltip } from "@/components/ui/tooltip";
import {
  membersByUserId,
  useWorkspaceMembers,
} from "@/hooks/api/use-workspace-members";
import { useAppStore } from "@/lib/store";
import type { WorkspaceMember } from "@/types/workspace";
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
  const workspaceId = useAppStore((s) => s.workspaceId);
  const { data: members } = useWorkspaceMembers(workspaceId);

  const memberLookup = useMemo(() => membersByUserId(members), [members]);

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
  const overflowMembers = visible.slice(maxVisible);

  return (
    <div
      className={cn("flex items-center -space-x-1.5", className)}
      role="group"
      aria-label={`${visible.length} active collaborators`}
    >
      {head.map((member) => (
        <PresenceAvatar
          key={member.user_id}
          member={member}
          detail={memberLookup.get(member.user_id)}
        />
      ))}
      {overflow > 0 && (
        <Tooltip
          content={
            <ul className="m-0 list-none p-0 text-2xs">
              {overflowMembers.map((m) => (
                <li key={m.user_id}>{m.user_name}</li>
              ))}
            </ul>
          }
        >
          <div className="flex h-7 w-7 items-center justify-center rounded-full border border-background bg-surface-raised text-2xs font-semibold text-foreground-strong dark:bg-surface-base dark:text-foreground">
            +{overflow}
          </div>
        </Tooltip>
      )}
    </div>
  );
}

function PresenceAvatar({
  member,
  detail,
}: {
  member: PresenceInfo;
  detail: WorkspaceMember | undefined;
}) {
  const initials = initialsFor(member.user_name);
  const color = colorFor(member.user_id);
  return (
    <Tooltip content={<AvatarDetail member={member} detail={detail} />}>
      <div
        className="flex h-7 w-7 items-center justify-center rounded-full border border-background text-2xs font-semibold text-white"
        style={{ backgroundColor: color }}
      >
        {initials}
      </div>
    </Tooltip>
  );
}

function AvatarDetail({
  member,
  detail,
}: {
  member: PresenceInfo;
  detail: WorkspaceMember | undefined;
}) {
  return (
    <div className="text-2xs leading-tight">
      <div className="font-semibold">{member.user_name}</div>
      {detail?.email && (
        <div className="mt-0.5 opacity-80">{detail.email}</div>
      )}
      {detail?.role && (
        <div className="mt-0.5 opacity-60">{detail.role}</div>
      )}
    </div>
  );
}

/** First letter of each whitespace-separated name part, max two. */
function initialsFor(name: string): string {
  const parts = name.trim().split(/\s+/).slice(0, 2);
  return parts.map((p) => p[0]?.toUpperCase() ?? "").join("");
}
