"use client";

import { useState, useEffect, useCallback } from "react";
import { toast } from "sonner";
import { Spinner } from "@/components/ui/spinner";
import { FormInput } from "@/components/ui/form-input";
import { Button } from "@/components/ui/button";
import { SettingsSection } from "@/components/settings/settings-section";
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

  const hasChanges = editName.trim() !== (workspace?.name ?? "");

  return (
    <SettingsSection
      title="Workspace Settings"
      description="Manage workspace details and team members."
      actions={
        <Button
          variant="primary"
          size="sm"
          onClick={handleSave}
          disabled={!hasChanges || saving}
        >
          {saving ? "Saving..." : "Save"}
        </Button>
      }
    >
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
            <FormInput
              value={editName}
              onChange={(e) => setEditName(e.target.value)}
            />
          </div>
          <div>
            <label className="mb-1 block text-xs font-medium text-zinc-600 dark:text-zinc-400">
              Slug
            </label>
            <FormInput
              value={workspace?.slug ?? ""}
              readOnly
              className="bg-zinc-50 font-mono text-zinc-500 dark:bg-zinc-800/50 dark:text-zinc-500"
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
        </div>
      </section>

      {/* ── Members ────────────────────────────────────────────── */}
      <MembersTable wsId={wsId} members={members} onReload={load} />
    </SettingsSection>
  );
}
