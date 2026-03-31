"use client";

import { useState, useEffect, useCallback } from "react";
import { toast } from "sonner";
import { Spinner } from "@/components/ui/spinner";
import { getWorkspace, updateWorkspace, listMembers } from "@/lib/api/workspaces";
import { getWorkspaceId, setWorkspaceName } from "@/lib/workspace";
import { MembersTable } from "@/components/workspace/members-table";
import type { Workspace, WorkspaceMember } from "@/types/workspace";

export default function WorkspaceSettingsPage() {
  const wsId = getWorkspaceId();

  const [workspace, setWorkspace] = useState<Workspace | null>(null);
  const [members, setMembers] = useState<WorkspaceMember[]>([]);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [editName, setEditName] = useState("");

  const load = useCallback(async () => {
    if (!wsId) return;
    try {
      const [ws, mems] = await Promise.all([
        getWorkspace(wsId),
        listMembers(wsId),
      ]);
      setWorkspace(ws);
      setEditName(ws.name);
      setMembers(mems);
    } catch {
      toast.error("Failed to load workspace");
    } finally {
      setLoading(false);
    }
  }, [wsId]);

  useEffect(() => {
    load();
  }, [load]);

  const handleSave = async () => {
    if (!wsId || !editName.trim()) return;
    setSaving(true);
    try {
      const updated = await updateWorkspace(wsId, { name: editName.trim() });
      setWorkspace(updated);
      setWorkspaceName(updated.name);
      toast.success("Workspace updated");
    } catch {
      toast.error("Failed to update workspace");
    } finally {
      setSaving(false);
    }
  };

  if (!wsId) {
    return (
      <div className="py-12 text-center text-sm text-zinc-400">
        No workspace selected. Switch to a workspace first.
      </div>
    );
  }

  if (loading) return <Spinner />;

  return (
    <div>
      <h1 className="text-xl font-semibold text-zinc-900 dark:text-zinc-100">
        Workspace Settings
      </h1>
      <p className="mt-1 text-sm text-zinc-500 dark:text-zinc-400">
        Manage workspace details and team members.
      </p>

      {/* ── General ────────────────────────────────────────────── */}
      <section className="mt-6">
        <h2 className="text-sm font-semibold text-zinc-800 dark:text-zinc-200">
          General
        </h2>
        <div className="mt-3 space-y-3">
          <div>
            <label className="mb-1 block text-xs font-medium text-zinc-600 dark:text-zinc-400">
              Name
            </label>
            <input
              value={editName}
              onChange={(e) => setEditName(e.target.value)}
              className="w-full max-w-sm rounded-md border border-zinc-200 bg-white px-3 py-2 text-sm text-zinc-700 focus:border-emerald-400 focus:ring-1 focus:ring-emerald-400/50 focus:outline-none dark:border-zinc-700 dark:bg-zinc-800 dark:text-zinc-300"
            />
          </div>
          <div>
            <label className="mb-1 block text-xs font-medium text-zinc-600 dark:text-zinc-400">
              Slug
            </label>
            <input
              value={workspace?.slug ?? ""}
              readOnly
              className="w-full max-w-sm rounded-md border border-zinc-200 bg-zinc-50 px-3 py-2 text-sm font-mono text-zinc-500 dark:border-zinc-700 dark:bg-zinc-800/50 dark:text-zinc-500"
            />
          </div>
          <div>
            <label className="mb-1 block text-xs font-medium text-zinc-600 dark:text-zinc-400">
              Created
            </label>
            <p className="text-sm text-zinc-500">
              {workspace?.created_at
                ? new Date(workspace.created_at).toLocaleDateString()
                : "-"}
            </p>
          </div>
          <button
            onClick={handleSave}
            disabled={saving || editName.trim() === workspace?.name}
            className="rounded-md bg-emerald-600 px-4 py-1.5 text-sm font-medium text-white hover:bg-emerald-700 disabled:opacity-50"
          >
            {saving ? "Saving..." : "Save"}
          </button>
        </div>
      </section>

      {/* ── Members ────────────────────────────────────────────── */}
      <MembersTable wsId={wsId} members={members} onReload={load} />
    </div>
  );
}
