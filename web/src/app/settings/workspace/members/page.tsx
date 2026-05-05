"use client";

import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useTranslations } from "next-intl";
import { toast } from "@/components/ui/toast";
import { Heading } from "@/components/ui/heading";

import { useAuth } from "@/hooks/use-auth";
import { listUsers, updateUserRole } from "@/lib/api";
import { Spinner } from "@/components/ui/spinner";
import { SettingsPageShell } from "@/components/layout/settings-page-shell";
import { EmptyState } from "@/components/ui/empty-state";
import { SkeletonCard, SkeletonTable } from "@/components/ui/skeleton";
import { PageStateView } from "@/components/layout/page-state-view";
import type { PageState } from "@/components/layout/page-state";
import { SettingsSelect } from "@/components/ui/form-input";
import { Avatar } from "@/components/ui/avatar";
import type { UserInfo } from "@/types/api";

const ROLES = ["admin", "designer", "viewer"] as const;
type KnownRole = (typeof ROLES)[number];

function isKnownRole(r: string): r is KnownRole {
  return r === "admin" || r === "designer" || r === "viewer";
}

const teamKeys = {
  all: ["team"] as const,
  members: () => [...teamKeys.all, "members"] as const,
};

export default function TeamPage() {
  const t = useTranslations("settings.workspace.members");
  const tCommon = useTranslations("common");
  const tRoles = useTranslations("settings.workspace.roles");
  const { user, loading: authLoading, authEnabled, isAdmin } = useAuth();
  const qc = useQueryClient();
  const [updatingId, setUpdatingId] = useState<string | null>(null);

  const query = useQuery({
    queryKey: teamKeys.members(),
    queryFn: async () => {
      const page = await listUsers({ limit: 100 });
      return page.items;
    },
    enabled: !authLoading && authEnabled,
  });

  const updateRoleMutation = useMutation({
    mutationFn: ({ userId, role }: { userId: string; role: string }) =>
      updateUserRole(userId, role),
    onMutate: ({ userId }) => setUpdatingId(userId),
    onSuccess: ({ user: updated }) => {
      qc.setQueryData<UserInfo[]>(teamKeys.members(), (prev) =>
        prev?.map((u) => (u.id === updated.id ? updated : u)) ?? prev,
      );
    },
    onError: () => toast.error(t("toast.updateRoleFailed")),
    onSettled: () => setUpdatingId(null),
  });

  if (authLoading) {
    return (
      <SettingsPageShell title={t("title")} subtitle={t("description")}>
        <div className="space-y-4">
          <SkeletonCard />
          <SkeletonTable rows={4} cols={3} />
        </div>
      </SettingsPageShell>
    );
  }

  if (!authEnabled) {
    return (
      <SettingsPageShell title={t("title")} subtitle={t("description")}>
        <div className="py-12">
          <EmptyState
            title={t("authRequired")}
            description={t("authRequiredDescription")}
          />
        </div>
      </SettingsPageShell>
    );
  }

  const users = query.data ?? [];
  const handleRoleChange = (userId: string, role: string) =>
    updateRoleMutation.mutate({ userId, role });

  const pageState: PageState = query.isLoading
    ? { kind: "loading" }
    : query.isError
      ? { kind: "error", onRetry: () => void query.refetch() }
      : { kind: "data" };

  return (
    <SettingsPageShell title={t("title")} subtitle={t("description")}>
      <PageStateView
        state={pageState}
        skeleton={
          <div className="space-y-4">
            <SkeletonCard />
            <SkeletonTable rows={4} cols={3} />
          </div>
        }
        error={{
          title: tCommon("loadError.title"),
          description: tCommon("loadError.description"),
          retryLabel: tCommon("retry"),
        }}
      >
      <div className="space-y-6">
        {/* Role Descriptions */}
        <section className="rounded-lg border border-divider bg-surface-base">
          <div className="border-b border-divider-soft px-6 py-4">
            <Heading level={2} size={6}>
              {t("rolesHeading")}
            </Heading>
          </div>
          <div className="divide-y divide-divider-soft">
            {ROLES.map((role) => (
              <div key={role} className="flex items-start gap-3 px-6 py-3">
                <RoleBadge role={role} roleLabel={tRoles(role)} />
                <p className="text-xs text-foreground-muted">
                  {t(`roleDescriptions.${role}`)}
                </p>
              </div>
            ))}
          </div>
        </section>

        {/* Members Table */}
        <section className="rounded-lg border border-divider bg-surface-base">
          <div className="border-b border-divider-soft px-6 py-4">
            <Heading level={2} size={6}>
              {t("membersHeading")}
              <span className="ms-2 text-xs font-normal text-foreground-muted">
                {users.length}
              </span>
            </Heading>
          </div>
          <div className="overflow-x-auto">
            <table className="w-full text-start text-sm">
              <thead>
                <tr className="border-b border-divider-soft">
                  <th scope="col" className="py-3 pe-6 text-xs font-medium text-foreground-muted">
                    {t("column.user")}
                  </th>
                  <th scope="col" className="py-3 pe-6 text-xs font-medium text-foreground-muted">
                    {t("column.email")}
                  </th>
                  <th scope="col" className="py-3 pe-6 text-xs font-medium text-foreground-muted">
                    {t("column.role")}
                  </th>
                </tr>
              </thead>
              <tbody className="divide-y divide-divider-soft">
                {users.map((member) => {
                  const isMe = member.id === user?.sub;
                  const displayName = member.name ?? member.email;
                  return (
                    <tr key={member.id}>
                      <td className="py-3 pe-6">
                        <div className="flex items-center gap-2.5">
                          <Avatar
                            src={member.picture}
                            name={displayName}
                            size="sm"
                          />
                          <span className="font-medium text-foreground-strong">
                            {displayName}
                          </span>
                          {isMe && (
                            <span className="rounded bg-surface-inset px-1.5 py-0.5 text-2xs text-foreground-muted">
                              {t("you")}
                            </span>
                          )}
                        </div>
                      </td>
                      <td className="py-3 pe-6 text-foreground">
                        {member.email}
                      </td>
                      <td className="py-3 pe-6">
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
                                className="absolute end-1.5 top-1 -translate-y-1/2 text-concept-foreground"
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
        <section className="rounded-lg border border-divider bg-surface-base p-6">
          <Heading level={2} size={6}>
            {t("addingMembers.heading")}
          </Heading>
          <p className="mt-1 text-xs text-foreground-muted">
            {t("addingMembers.description")}
          </p>
        </section>
      </div>
      </PageStateView>
    </SettingsPageShell>
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
      ? "bg-concept-surface text-concept-foreground"
      : role === "designer"
        ? "bg-success-surface text-success-foreground"
        : "bg-surface-inset text-foreground-muted";

  return (
    <span
      className={`inline-block shrink-0 rounded-full px-2 py-0.5 text-xs font-medium ${styles}`}
    >
      {roleLabel ?? role}
    </span>
  );
}
