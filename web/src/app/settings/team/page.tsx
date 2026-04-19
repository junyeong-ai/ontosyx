"use client";

import { useCallback, useEffect, useState } from "react";
import { useTranslations } from "next-intl";
import { useAuth } from "@/lib/use-auth";
import { listUsers, updateUserRole } from "@/lib/api";
import { Spinner } from "@/components/ui/spinner";
import { SettingsSelect } from "@/components/ui/form-input";
import { Avatar } from "@/components/ui/avatar";
import type { UserInfo } from "@/types/api";

const ROLES = ["admin", "designer", "viewer"] as const;
type KnownRole = (typeof ROLES)[number];

function isKnownRole(r: string): r is KnownRole {
  return r === "admin" || r === "designer" || r === "viewer";
}

export default function TeamPage() {
  const t = useTranslations("settings.team");
  const tRoles = useTranslations("settings.roles");
  const { user, loading: authLoading, authEnabled, isAdmin } = useAuth();
  const [users, setUsers] = useState<UserInfo[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [updatingId, setUpdatingId] = useState<string | null>(null);

  const fetchUsers = useCallback(async () => {
    try {
      setError(null);
      const page = await listUsers({ limit: 100 });
      setUsers(page.items);
    } catch (e) {
      setError(e instanceof Error ? e.message : t("loadError"));
    } finally {
      setLoading(false);
    }
  }, [t]);

  useEffect(() => {
    if (!authLoading && authEnabled) {
      fetchUsers();
    } else if (!authLoading) {
      setLoading(false);
    }
  }, [authLoading, authEnabled, fetchUsers]);

  const handleRoleChange = async (userId: string, newRole: string) => {
    setUpdatingId(userId);
    try {
      const { user: updated } = await updateUserRole(userId, newRole);
      setUsers((prev) =>
        prev.map((u) => (u.id === updated.id ? updated : u)),
      );
    } catch (e) {
      setError(e instanceof Error ? e.message : t("updateRoleError"));
    } finally {
      setUpdatingId(null);
    }
  };

  if (authLoading || loading) {
    return (
      <div className="flex items-center justify-center py-20">
        <Spinner size="lg" className="text-emerald-500" />
      </div>
    );
  }

  if (!authEnabled) {
    return (
      <div>
        <h1 className="text-xl font-semibold text-zinc-900 dark:text-zinc-100">
          {t("title")}
        </h1>
        <div className="mt-6 rounded-lg border border-zinc-200 bg-white p-6 dark:border-zinc-800 dark:bg-zinc-900">
          <p className="text-sm text-zinc-500 dark:text-muted-foreground">
            {t("authRequired")}
          </p>
        </div>
      </div>
    );
  }

  return (
    <div>
      <h1 className="text-xl font-semibold text-zinc-900 dark:text-zinc-100">
        {t("title")}
      </h1>
      <p className="mt-1 text-sm text-zinc-500 dark:text-muted-foreground">
        {t("description")}
      </p>

      <div className="mt-6 space-y-6">
        {/* Role Descriptions */}
        <section className="rounded-lg border border-zinc-200 bg-white dark:border-zinc-800 dark:bg-zinc-900">
          <div className="border-b border-zinc-100 px-6 py-4 dark:border-zinc-800">
            <h2 className="text-sm font-semibold text-zinc-900 dark:text-zinc-100">
              {t("rolesHeading")}
            </h2>
          </div>
          <div className="divide-y divide-zinc-100 dark:divide-zinc-800">
            {ROLES.map((role) => (
              <div key={role} className="flex items-start gap-3 px-6 py-3">
                <RoleBadge role={role} roleLabel={tRoles(role)} />
                <p className="text-xs text-zinc-500 dark:text-muted-foreground">
                  {t(`roleDescriptions.${role}`)}
                </p>
              </div>
            ))}
          </div>
        </section>

        {/* Error */}
        {error && (
          <div className="rounded-lg border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-700 dark:border-red-800 dark:bg-red-900/20 dark:text-red-400">
            {error}
          </div>
        )}

        {/* Members Table */}
        <section className="rounded-lg border border-zinc-200 bg-white dark:border-zinc-800 dark:bg-zinc-900">
          <div className="border-b border-zinc-100 px-6 py-4 dark:border-zinc-800">
            <h2 className="text-sm font-semibold text-zinc-900 dark:text-zinc-100">
              {t("membersHeading")}
              <span className="ml-2 text-xs font-normal text-muted-foreground">
                {users.length}
              </span>
            </h2>
          </div>
          <div className="overflow-x-auto">
            <table className="w-full text-left text-sm">
              <thead>
                <tr className="border-b border-zinc-100 dark:border-zinc-800">
                  <th scope="col" className="py-3 pr-6 text-xs font-medium text-zinc-500 dark:text-muted-foreground">
                    {t("column.user")}
                  </th>
                  <th scope="col" className="py-3 pr-6 text-xs font-medium text-zinc-500 dark:text-muted-foreground">
                    {t("column.email")}
                  </th>
                  <th scope="col" className="py-3 pr-6 text-xs font-medium text-zinc-500 dark:text-muted-foreground">
                    {t("column.role")}
                  </th>
                </tr>
              </thead>
              <tbody className="divide-y divide-zinc-100 dark:divide-zinc-800">
                {users.map((member) => {
                  const isMe = member.id === user?.sub;
                  const displayName = member.name ?? member.email;
                  return (
                    <tr key={member.id}>
                      <td className="py-3 pr-6">
                        <div className="flex items-center gap-2.5">
                          <Avatar
                            src={member.picture}
                            name={displayName}
                            size="sm"
                          />
                          <span className="font-medium text-zinc-900 dark:text-zinc-100">
                            {displayName}
                          </span>
                          {isMe && (
                            <span className="rounded bg-zinc-100 px-1.5 py-0.5 text-[10px] text-muted-foreground dark:bg-zinc-800">
                              {t("you")}
                            </span>
                          )}
                        </div>
                      </td>
                      <td className="py-3 pr-6 text-zinc-600 dark:text-muted-foreground">
                        {member.email}
                      </td>
                      <td className="py-3 pr-6">
                        {isAdmin && !isMe ? (
                          <div className="relative">
                            <SettingsSelect
                              label={t("roleSelectLabel")}
                              hideLabel
                              value={member.role}
                              onChange={(e) =>
                                handleRoleChange(member.id, e.target.value)
                              }
                              disabled={updatingId === member.id}
                              aria-label={t("changeRoleAria", { name: displayName })}
                              className="capitalize"
                            >
                              {ROLES.map((r) => (
                                <option key={r} value={r}>
                                  {tRoles(r)}
                                </option>
                              ))}
                            </SettingsSelect>
                            {updatingId === member.id && (
                              <Spinner
                                size="sm"
                                className="absolute right-1.5 top-1/2 -translate-y-1/2 text-indigo-500"
                              />
                            )}
                          </div>
                        ) : (
                          <RoleBadge
                            role={member.role}
                            roleLabel={
                              member.role && isKnownRole(member.role)
                                ? tRoles(member.role)
                                : member.role
                            }
                          />
                        )}
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        </section>

        {/* Invite Note */}
        <section className="rounded-lg border border-zinc-200 bg-white p-6 dark:border-zinc-800 dark:bg-zinc-900">
          <h2 className="text-sm font-semibold text-zinc-900 dark:text-zinc-100">
            {t("addingMembers.heading")}
          </h2>
          <p className="mt-1 text-xs text-zinc-500 dark:text-muted-foreground">
            {t("addingMembers.description")}
          </p>
        </section>
      </div>
    </div>
  );
}

function RoleBadge({
  role,
  roleLabel,
}: {
  role?: string;
  roleLabel?: string;
}) {
  if (!role) return null;

  const styles =
    role === "admin"
      ? "bg-indigo-100 text-indigo-700 dark:bg-indigo-900/30 dark:text-indigo-400"
      : role === "designer"
        ? "bg-emerald-100 text-emerald-700 dark:bg-emerald-900/30 dark:text-emerald-400"
        : "bg-zinc-100 text-zinc-600 dark:bg-zinc-800 dark:text-muted-foreground";

  return (
    <span
      className={`inline-block shrink-0 rounded-full px-2 py-0.5 text-xs font-medium ${styles}`}
    >
      {roleLabel ?? role}
    </span>
  );
}
