"use client";

import { useState } from "react";
import { toast } from "sonner";
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
  owner: "bg-amber-100 text-amber-700 dark:bg-amber-900/50 dark:text-amber-400",
  admin: "bg-indigo-100 text-indigo-700 dark:bg-indigo-900/50 dark:text-indigo-400",
  member: "bg-zinc-100 text-zinc-600 dark:bg-zinc-800 dark:text-zinc-400",
  viewer: "bg-zinc-100 text-zinc-500 dark:bg-zinc-800 dark:text-zinc-500",
};

const ROLES = ["admin", "member", "viewer"];

interface Props {
  wsId: string;
  members: WorkspaceMember[];
  onReload: () => void;
}

export function MembersTable({ wsId, members, onReload }: Props) {
  const [showAdd, setShowAdd] = useState(false);
  const [users, setUsers] = useState<UserInfo[]>([]);
  const [usersLoading, setUsersLoading] = useState(false);
  const [confirmRemove, setConfirmRemove] = useState<string | null>(null);

  const handleRoleChange = async (userId: string, role: string) => {
    try {
      await updateMemberRole(wsId, userId, role);
      onReload();
      toast.success("Role updated");
    } catch {
      toast.error("Failed to update role");
    }
  };

  const handleRemove = async (userId: string) => {
    try {
      await removeMember(wsId, userId);
      setConfirmRemove(null);
      onReload();
      toast.success("Member removed");
    } catch {
      toast.error("Failed to remove member");
    }
  };

  const handleAdd = async (userId: string) => {
    try {
      await addMember(wsId, { user_id: userId, role: "member" });
      setShowAdd(false);
      onReload();
      toast.success("Member added");
    } catch {
      toast.error("Failed to add member");
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
      toast.error("Failed to load users");
    } finally {
      setUsersLoading(false);
    }
  };

  return (
    <section className="mt-8">
      <div className="flex items-center justify-between">
        <h2 className="text-sm font-semibold text-zinc-800 dark:text-zinc-200">
          Members
        </h2>
        <button
          onClick={openAdd}
          className="rounded-md bg-indigo-600 px-3 py-1 text-xs font-medium text-white hover:bg-indigo-700"
        >
          Add Member
        </button>
      </div>

      {showAdd && (
        <div className="mt-3 rounded-md border border-zinc-200 bg-zinc-50 p-3 dark:border-zinc-700 dark:bg-zinc-800/50">
          <div className="mb-2 flex items-center justify-between">
            <span className="text-xs font-medium text-zinc-600 dark:text-zinc-400">
              Select a user to add
            </span>
            <button
              onClick={() => setShowAdd(false)}
              className="text-xs text-zinc-400 hover:text-zinc-600"
            >
              Cancel
            </button>
          </div>
          {usersLoading ? (
            <Spinner size="sm" className="mx-auto" />
          ) : users.length === 0 ? (
            <p className="text-xs text-zinc-400">No users available</p>
          ) : (
            <div className="max-h-40 space-y-1 overflow-auto">
              {users.map((u) => (
                <button
                  key={u.id}
                  onClick={() => handleAdd(u.id)}
                  className="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-xs hover:bg-white dark:hover:bg-zinc-700"
                >
                  <span className="text-zinc-700 dark:text-zinc-300">
                    {u.name || u.email}
                  </span>
                  {u.name && <span className="text-zinc-400">{u.email}</span>}
                </button>
              ))}
            </div>
          )}
        </div>
      )}

      <div className="mt-3">
        <table className="w-full text-sm">
          <thead>
            <tr className="border-b border-zinc-200 text-left text-xs font-medium uppercase text-zinc-500 dark:border-zinc-700">
              <th className="py-2">User</th>
              <th className="py-2">Role</th>
              <th className="py-2">Joined</th>
              <th className="py-2 text-right">Actions</th>
            </tr>
          </thead>
          <tbody>
            {members.map((m) => (
              <tr
                key={m.user_id}
                className="border-b border-zinc-100 dark:border-zinc-800"
              >
                <td className="py-2 text-zinc-900 dark:text-zinc-100">
                  {m.name || m.email || m.user_id.slice(0, 8)}
                </td>
                <td className="py-2">
                  {m.role === "owner" ? (
                    <span
                      className={`rounded px-1.5 py-0.5 text-[10px] font-medium ${ROLE_COLORS.owner}`}
                    >
                      owner
                    </span>
                  ) : (
                    <select
                      value={m.role}
                      onChange={(e) =>
                        handleRoleChange(m.user_id, e.target.value)
                      }
                      className="rounded border border-zinc-200 bg-white px-1.5 py-0.5 text-xs dark:border-zinc-700 dark:bg-zinc-800 dark:text-zinc-300"
                    >
                      {ROLES.map((r) => (
                        <option key={r} value={r}>
                          {r}
                        </option>
                      ))}
                    </select>
                  )}
                </td>
                <td className="py-2 text-zinc-500">
                  {m.joined_at
                    ? new Date(m.joined_at).toLocaleDateString()
                    : "-"}
                </td>
                <td className="py-2 text-right">
                  {m.role !== "owner" &&
                    (confirmRemove === m.user_id ? (
                      <span className="space-x-1">
                        <button
                          onClick={() => handleRemove(m.user_id)}
                          className="rounded bg-red-600 px-2 py-0.5 text-[10px] font-medium text-white hover:bg-red-700"
                        >
                          Confirm
                        </button>
                        <button
                          onClick={() => setConfirmRemove(null)}
                          className="rounded px-2 py-0.5 text-[10px] text-zinc-500 hover:bg-zinc-100 dark:hover:bg-zinc-800"
                        >
                          Cancel
                        </button>
                      </span>
                    ) : (
                      <button
                        onClick={() => setConfirmRemove(m.user_id)}
                        className="rounded px-2 py-0.5 text-[10px] text-red-500 hover:bg-red-50 dark:hover:bg-red-950/30"
                      >
                        Remove
                      </button>
                    ))}
                </td>
              </tr>
            ))}
            {members.length === 0 && (
              <tr>
                <td colSpan={4} className="py-8 text-center text-zinc-400">
                  No members
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </div>
    </section>
  );
}
