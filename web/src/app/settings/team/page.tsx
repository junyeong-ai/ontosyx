"use client";

import { useCallback, useEffect, useState } from "react";
import { useAuth } from "@/lib/use-auth";
import { listUsers, updateUserRole } from "@/lib/api";
import { Spinner } from "@/components/ui/spinner";
import type { UserInfo } from "@/types/api";

const ROLE_DESCRIPTIONS: Record<string, string> = {
  admin: "Full access to all features including system configuration and user management",
  designer: "Create and edit ontology designs, run queries, and manage projects",
  viewer: "View ontologies, run chat queries, and browse query history",
};

const ROLES = ["admin", "designer", "viewer"] as const;

export default function TeamPage() {
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
      setError(e instanceof Error ? e.message : "Failed to load users");
    } finally {
      setLoading(false);
    }
  }, []);

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
      setError(e instanceof Error ? e.message : "Failed to update role");
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
          Team
        </h1>
        <div className="mt-6 rounded-lg border border-zinc-200 bg-white p-6 dark:border-zinc-800 dark:bg-zinc-900">
          <p className="text-sm text-zinc-500 dark:text-zinc-400">
            Team management is available when authentication is enabled.
          </p>
        </div>
      </div>
    );
  }

  return (
    <div>
      <h1 className="text-xl font-semibold text-zinc-900 dark:text-zinc-100">
        Team
      </h1>
      <p className="mt-1 text-sm text-zinc-500 dark:text-zinc-400">
        Manage team members and their roles.
      </p>

      <div className="mt-6 space-y-6">
        {/* Role Descriptions */}
        <section className="rounded-lg border border-zinc-200 bg-white dark:border-zinc-800 dark:bg-zinc-900">
          <div className="border-b border-zinc-100 px-6 py-4 dark:border-zinc-800">
            <h2 className="text-sm font-semibold text-zinc-900 dark:text-zinc-100">
              Roles
            </h2>
          </div>
          <div className="divide-y divide-zinc-100 dark:divide-zinc-800">
            {Object.entries(ROLE_DESCRIPTIONS).map(([role, description]) => (
              <div key={role} className="flex items-start gap-3 px-6 py-3">
                <RoleBadge role={role} />
                <p className="text-xs text-zinc-500 dark:text-zinc-400">
                  {description}
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
              Members
              <span className="ml-2 text-xs font-normal text-zinc-400">
                {users.length}
              </span>
            </h2>
          </div>
          <div className="overflow-x-auto">
            <table className="w-full text-left text-sm">
              <thead>
                <tr className="border-b border-zinc-100 dark:border-zinc-800">
                  <th className="px-6 py-2.5 text-xs font-medium text-zinc-500 dark:text-zinc-400">
                    User
                  </th>
                  <th className="px-6 py-2.5 text-xs font-medium text-zinc-500 dark:text-zinc-400">
                    Email
                  </th>
                  <th className="px-6 py-2.5 text-xs font-medium text-zinc-500 dark:text-zinc-400">
                    Role
                  </th>
                </tr>
              </thead>
              <tbody className="divide-y divide-zinc-100 dark:divide-zinc-800">
                {users.map((member) => {
                  const isMe = member.id === user?.sub;
                  return (
                    <tr key={member.id}>
                      <td className="px-6 py-3">
                        <div className="flex items-center gap-2.5">
                          {member.picture ? (
                            <img
                              src={member.picture}
                              alt={member.name ?? member.email}
                              className="h-7 w-7 rounded-full"
                              referrerPolicy="no-referrer"
                            />
                          ) : (
                            <div className="flex h-7 w-7 items-center justify-center rounded-full bg-indigo-600 text-[10px] font-semibold text-white">
                              {(member.name ?? member.email)
                                .split(" ")
                                .map((n) => n[0])
                                .join("")
                                .toUpperCase()
                                .slice(0, 2)}
                            </div>
                          )}
                          <span className="font-medium text-zinc-900 dark:text-zinc-100">
                            {member.name ?? member.email}
                          </span>
                          {isMe && (
                            <span className="rounded bg-zinc-100 px-1.5 py-0.5 text-[10px] text-zinc-500 dark:bg-zinc-800">
                              you
                            </span>
                          )}
                        </div>
                      </td>
                      <td className="px-6 py-3 text-zinc-600 dark:text-zinc-400">
                        {member.email}
                      </td>
                      <td className="px-6 py-3">
                        {isAdmin && !isMe ? (
                          <div className="relative">
                            <select
                              value={member.role}
                              onChange={(e) =>
                                handleRoleChange(member.id, e.target.value)
                              }
                              disabled={updatingId === member.id}
                              aria-label={`Change role for ${member.name ?? member.email}`}
                              className="appearance-none rounded-md border border-zinc-200 bg-white px-2.5 py-1 pr-7 text-xs font-medium capitalize text-zinc-700 focus:border-indigo-500 focus:outline-none focus:ring-1 focus:ring-indigo-500 disabled:opacity-50 dark:border-zinc-700 dark:bg-zinc-800 dark:text-zinc-300"
                            >
                              {ROLES.map((r) => (
                                <option key={r} value={r}>
                                  {r}
                                </option>
                              ))}
                            </select>
                            {updatingId === member.id && (
                              <Spinner
                                size="sm"
                                className="absolute right-1.5 top-1/2 -translate-y-1/2 text-indigo-500"
                              />
                            )}
                          </div>
                        ) : (
                          <RoleBadge role={member.role} />
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
            Adding Members
          </h2>
          <p className="mt-1 text-xs text-zinc-500 dark:text-zinc-400">
            Users are automatically added on first login via SSO. Share the
            platform URL with your team members and ask them to sign in with
            their Google account.
          </p>
        </section>
      </div>
    </div>
  );
}

function RoleBadge({ role }: { role?: string }) {
  if (!role) return null;

  const styles =
    role === "admin"
      ? "bg-indigo-100 text-indigo-700 dark:bg-indigo-900/30 dark:text-indigo-400"
      : role === "designer"
        ? "bg-emerald-100 text-emerald-700 dark:bg-emerald-900/30 dark:text-emerald-400"
        : "bg-zinc-100 text-zinc-600 dark:bg-zinc-800 dark:text-zinc-400";

  return (
    <span
      className={`inline-block shrink-0 rounded-full px-2 py-0.5 text-xs font-medium capitalize ${styles}`}
    >
      {role}
    </span>
  );
}
