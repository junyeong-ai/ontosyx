"use client";

import { useState } from "react";
import { useTranslations } from "next-intl";
import { toast } from "@/components/ui/toast";
import { Heading } from "@/components/ui/heading";
import { FormSelect } from "@/components/ui/form-input";
import { Spinner } from "@/components/ui/spinner";
import {
  addMember,
  updateMemberRole,
  removeMember,
} from "@/lib/api/workspaces";
import { listUsers } from "@/lib/api/admin";
import type { WorkspaceMember } from "@/types/workspace";
import type { UserInfo } from "@/types/admin";

const ROLE_COLORS: Record<string, string> = {
  owner: "bg-warning-surface text-warning-foreground",
  admin: "bg-concept-surface text-concept-foreground",
  member: "bg-surface-inset text-foreground",
  viewer: "bg-surface-inset text-foreground-muted",
};

const ROLES = ["admin", "member", "viewer"];

interface Props {
  wsId: string;
  members: WorkspaceMember[];
  onReload: () => void;
}

export function MembersTable({ wsId, members, onReload }: Props) {
  const t = useTranslations("workspaceDialog.members");
  const tCommon = useTranslations("common");
  const [showAdd, setShowAdd] = useState(false);
  const [users, setUsers] = useState<UserInfo[]>([]);
  const [usersLoading, setUsersLoading] = useState(false);
  const [confirmRemove, setConfirmRemove] = useState<string | null>(null);

  const handleRoleChange = async (userId: string, role: string) => {
    try {
      await updateMemberRole(wsId, userId, role);
      onReload();
      toast.success(t("toast.roleUpdated"));
    } catch {
      toast.error(t("toast.roleUpdateError"));
    }
  };

  const handleRemove = async (userId: string) => {
    try {
      await removeMember(wsId, userId);
      setConfirmRemove(null);
      onReload();
      toast.success(t("toast.memberRemoved"));
    } catch {
      toast.error(t("toast.memberRemoveError"));
    }
  };

  const handleAdd = async (userId: string) => {
    try {
      await addMember(wsId, { user_id: userId, role: "member" });
      setShowAdd(false);
      onReload();
      toast.success(t("toast.memberAdded"));
    } catch {
      toast.error(t("toast.memberAddError"));
    }
  };

  const openAdd = async () => {
    setShowAdd(true);
    setUsersLoading(true);
    try {
      const page = await listUsers({ limit: 100 });
      const ids = new Set(members.map((m) => m.user_id));
      setUsers(page.items.filter((u) => !ids.has(u.id)));
    } catch {
      toast.error(t("toast.loadUsersError"));
    } finally {
      setUsersLoading(false);
    }
  };

  return (
    <section className="mt-8">
      <div className="flex items-center justify-between">
        <Heading level={2} size={6}>
          {t("heading")}
        </Heading>
        <button type="button"
          onClick={openAdd}
          className="rounded-md bg-concept-foreground px-3 py-1 text-xs font-medium text-foreground-onbrand hover:bg-concept-foreground"
        >
          {t("add")}
        </button>
      </div>

      {showAdd && (
        <div className="mt-3 rounded-md border border-divider bg-surface-raised p-3">
          <div className="mb-2 flex items-center justify-between">
            <span className="text-xs font-medium text-foreground">
              {t("selectUserPrompt")}
            </span>
            <button type="button"
              onClick={() => setShowAdd(false)}
              className="text-xs text-foreground-muted hover:text-foreground"
            >
              {tCommon("cancel")}
            </button>
          </div>
          {usersLoading ? (
            <Spinner size="sm" className="mx-auto" />
          ) : users.length === 0 ? (
            <p className="text-xs text-foreground-muted">{t("noUsersAvailable")}</p>
          ) : (
            <div className="max-h-40 space-y-1 overflow-auto">
              {users.map((u) => (
                <button type="button"
                  key={u.id}
                  onClick={() => handleAdd(u.id)}
                  className="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-start text-xs hover:bg-surface-base"
                >
                  <span className="text-foreground">
                    {u.name || u.email}
                  </span>
                  {u.name && <span className="text-foreground-muted">{u.email}</span>}
                </button>
              ))}
            </div>
          )}
        </div>
      )}

      <div className="mt-3">
        <table className="w-full text-sm">
          <thead>
            <tr className="border-b border-divider text-start text-xs font-medium uppercase text-foreground-muted">
              <th className="py-2">{t("column.user")}</th>
              <th className="py-2">{t("column.role")}</th>
              <th className="py-2">{t("column.joined")}</th>
              <th className="py-2 text-end">{t("column.actions")}</th>
            </tr>
          </thead>
          <tbody>
            {members.map((m) => (
              <tr
                key={m.user_id}
                className="border-b border-divider-soft"
              >
                <td className="py-2 text-foreground-strong">
                  {m.name || m.email}
                </td>
                <td className="py-2">
                  {m.role === "owner" ? (
                    <span
                      className={`rounded px-1.5 py-0.5 text-2xs font-medium ${ROLE_COLORS.owner}`}
                    >
                      {t("ownerBadge")}
                    </span>
                  ) : (
                    <FormSelect
                      value={m.role}
                      onChange={(e) =>
                        handleRoleChange(m.user_id, e.target.value)
                      }
                      density="compact"
                    >
                      {ROLES.map((r) => (
                        <option key={r} value={r}>
                          {r}
                        </option>
                      ))}
                    </FormSelect>
                  )}
                </td>
                <td className="py-2 text-foreground-muted">
                  {m.joined_at
                    ? new Date(m.joined_at).toLocaleDateString()
                    : t("dateFallback")}
                </td>
                <td className="py-2 text-end">
                  {m.role !== "owner" &&
                    (confirmRemove === m.user_id ? (
                      <span className="space-x-1">
                        <button type="button"
                          onClick={() => handleRemove(m.user_id)}
                          className="rounded bg-danger-solid px-2 py-0.5 text-2xs font-medium text-foreground-on-accent hover:bg-danger-solid-hover"
                        >
                          {t("confirmRemove")}
                        </button>
                        <button type="button"
                          onClick={() => setConfirmRemove(null)}
                          className="rounded px-2 py-0.5 text-2xs text-foreground-muted hover:bg-surface-inset"
                        >
                          {tCommon("cancel")}
                        </button>
                      </span>
                    ) : (
                      <button type="button"
                        onClick={() => setConfirmRemove(m.user_id)}
                        className="rounded px-2 py-0.5 text-2xs text-danger-foreground hover:bg-danger-surface"
                      >
                        {t("remove")}
                      </button>
                    ))}
                </td>
              </tr>
            ))}
            {members.length === 0 && (
              <tr>
                <td colSpan={4} className="py-8 text-center text-foreground-muted">
                  {t("noMembers")}
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </div>
    </section>
  );
}
