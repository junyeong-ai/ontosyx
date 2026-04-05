"use client";

import { useState, useEffect, useCallback } from "react";
import { toast } from "sonner";
import { request } from "@/lib/api/client";
import { Spinner } from "@/components/ui/spinner";

interface ApprovalRequest {
  id: string;
  requester_id: string;
  action_type: string;
  resource_type: string;
  resource_id: string;
  status: string;
  reviewer_id: string | null;
  review_notes: string | null;
  expires_at: string;
  created_at: string;
}

export default function ApprovalsSettingsPage() {
  const [approvals, setApprovals] = useState<ApprovalRequest[]>([]);
  const [loading, setLoading] = useState(true);

  const load = useCallback(async () => {
    try {
      const data = await request<ApprovalRequest[]>("/approvals");
      setApprovals(Array.isArray(data) ? data : []);
    } catch {
      toast.error("Failed to load approvals");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => { load(); }, [load]);

  const handleReview = async (id: string, approved: boolean) => {
    try {
      await request(`/approvals/${id}/review`, {
        method: "POST",
        body: JSON.stringify({ approved, notes: null }),
      });
      toast.success(approved ? "Approved" : "Rejected");
      load();
    } catch {
      toast.error("Failed to review approval");
    }
  };

  if (loading) return <Spinner />;

  const pending = approvals.filter((a) => a.status === "pending");
  const resolved = approvals.filter((a) => a.status !== "pending");

  const statusBadge = (status: string) => {
    switch (status) {
      case "pending": return "bg-amber-100 text-amber-700 dark:bg-amber-900/30 dark:text-amber-400";
      case "approved": return "bg-emerald-100 text-emerald-700 dark:bg-emerald-900/30 dark:text-emerald-400";
      case "rejected": return "bg-red-100 text-red-700 dark:bg-red-900/30 dark:text-red-400";
      case "expired": return "bg-zinc-100 text-zinc-500 dark:bg-zinc-800 dark:text-zinc-400";
      default: return "bg-zinc-100 text-zinc-500";
    }
  };

  return (
    <div>
      <h1 className="text-xl font-semibold text-zinc-900 dark:text-zinc-100">
        Approval Queue
      </h1>
      <p className="mt-1 text-sm text-zinc-500 dark:text-zinc-400">
        Review and approve schema deployments and other gated operations.
      </p>

      {/* Pending approvals */}
      {pending.length > 0 && (
        <div className="mt-6">
          <h2 className="text-sm font-semibold text-zinc-700 dark:text-zinc-300">
            Pending ({pending.length})
          </h2>
          <div className="mt-2 space-y-3">
            {pending.map((a) => (
              <div
                key={a.id}
                className="flex items-center justify-between rounded-lg border border-amber-200 bg-amber-50 p-4 dark:border-amber-900 dark:bg-amber-950"
              >
                <div>
                  <div className="font-medium text-zinc-900 dark:text-zinc-100">
                    {a.action_type.replace(/_/g, " ")}
                  </div>
                  <div className="text-xs text-zinc-500">
                    {a.resource_type} {a.resource_id.slice(0, 8)}... | Expires{" "}
                    {new Date(a.expires_at).toLocaleDateString()}
                  </div>
                </div>
                <div className="flex gap-2">
                  <button
                    onClick={() => handleReview(a.id, true)}
                    className="rounded-md bg-emerald-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-emerald-700"
                  >
                    Approve
                  </button>
                  <button
                    onClick={() => handleReview(a.id, false)}
                    className="rounded-md bg-red-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-red-700"
                  >
                    Reject
                  </button>
                </div>
              </div>
            ))}
          </div>
        </div>
      )}

      {/* History */}
      <div className="mt-6">
        <h2 className="text-sm font-semibold text-zinc-700 dark:text-zinc-300">
          History
        </h2>
        <table className="mt-2 w-full text-sm">
          <thead>
            <tr className="border-b border-zinc-200 text-left text-xs font-medium uppercase text-zinc-500 dark:border-zinc-700">
              <th className="py-3 pr-6">Action</th>
              <th className="py-3 pr-6">Resource</th>
              <th className="py-3 pr-6">Status</th>
              <th className="py-3 pr-6">Date</th>
            </tr>
          </thead>
          <tbody>
            {resolved.map((a) => (
              <tr key={a.id} className="border-b border-zinc-100 dark:border-zinc-800">
                <td className="py-3 pr-6 text-zinc-900 dark:text-zinc-100">
                  {a.action_type.replace(/_/g, " ")}
                </td>
                <td className="py-3 pr-6 text-zinc-500">
                  {a.resource_type} {a.resource_id.slice(0, 8)}...
                </td>
                <td className="py-3 pr-6">
                  <span className={`rounded-full px-2 py-0.5 text-xs font-medium ${statusBadge(a.status)}`}>
                    {a.status}
                  </span>
                </td>
                <td className="py-3 pr-6 text-zinc-500">
                  {new Date(a.created_at).toLocaleDateString()}
                </td>
              </tr>
            ))}
            {approvals.length === 0 && (
              <tr>
                <td colSpan={4} className="py-8 text-center text-zinc-400">
                  No approval history
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </div>
    </div>
  );
}
