"use client";

import { useTranslations } from "next-intl";
import { useAuth } from "@/hooks/use-auth";
import { Spinner } from "@/components/ui/spinner";
import { Avatar } from "@/components/ui/avatar";
import { Card } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { SettingsPageShell } from "@/components/layout/settings-page-shell";

const ROLE_COLORS: Record<string, string> = {
  admin:
    "bg-concept-surface text-concept-foreground dark:bg-concept-foreground/30 dark:text-concept-foreground",
  designer:
    "bg-success-surface text-success-foreground",
  viewer:
    "bg-surface-inset text-foreground dark:text-muted-foreground",
};

export default function ProfilePage() {
  const t = useTranslations("settings.profile");
  const roleT = useTranslations("settings.roles");
  const authT = useTranslations("auth");
  const { user, loading, authEnabled } = useAuth();

  if (loading) {
    return (
      <div className="flex items-center justify-center py-20">
        <Spinner size="lg" className="text-brand-foreground" />
      </div>
    );
  }

  if (!authEnabled) {
    return (
      <SettingsPageShell title={t("title")}>
        <Card padding="lg">
          <div className="flex items-center gap-3">
            <div className="flex h-12 w-12 items-center justify-center rounded-full bg-surface-inset text-lg font-semibold text-foreground-muted">
              D
            </div>
            <div>
              <p className="text-sm font-medium text-foreground-strong">
                {t("devMode.userName")}
              </p>
              <p className="text-xs text-foreground-muted">
                {t("devMode.userEmail")}
              </p>
            </div>
          </div>
          <div className="mt-4 rounded-md border border-warning-border bg-warning-surface px-4 py-3 text-sm text-warning-foreground">
            {t("devMode.notice")}
          </div>
        </Card>
      </SettingsPageShell>
    );
  }

  if (!user) {
    return (
      <SettingsPageShell title={t("title")}>
        <Card padding="lg" className="text-center">
          <p className="text-sm text-foreground-muted">{t("notSignedIn")}</p>
          <a
            href="/login"
            className="mt-3 inline-block rounded-md bg-brand-solid px-4 py-2 text-sm font-medium text-foreground-onbrand hover:bg-brand-solid-hover"
          >
            {authT("signIn")}
          </a>
        </Card>
      </SettingsPageShell>
    );
  }

  const role = user.role;
  // Translated role label, or raw role string for an unknown value so
  // the UI still surfaces *something* instead of a blank pill.
  const roleLabel = role
    ? (roleHasTranslation(role) ? roleT(role as "admin" | "designer" | "viewer") : role)
    : undefined;
  const roleColor = role
    ? ROLE_COLORS[role] ??
      "bg-surface-inset text-foreground dark:text-muted-foreground"
    : undefined;

  return (
    <SettingsPageShell title={t("title")}>
      <div className="space-y-6">
        {/* Avatar & Identity */}
        <Card padding="lg">
          <div className="flex items-center gap-4">
            <Avatar src={user.picture} name={user.name} size="lg" />
            <div>
              <p className="text-lg font-semibold text-foreground-strong">
                {user.name}
              </p>
              <p className="text-sm text-foreground-muted">
                {user.email}
              </p>
              {roleLabel && roleColor && (
                <span
                  className={`mt-1 inline-block rounded-full px-2.5 py-0.5 text-xs font-medium ${roleColor}`}
                >
                  {roleLabel}
                </span>
              )}
            </div>
          </div>
        </Card>

        {/* Account Details */}
        <Card padding="none">
          <Card.Header className="px-6 py-4">
            <Card.Title>{t("accountDetails")}</Card.Title>
          </Card.Header>
          <div className="divide-y divide-divider-soft">
            <DetailRow label={t("field.name")} value={user.name} />
            <DetailRow label={t("field.email")} value={user.email} />
            <DetailRow
              label={t("field.signInProvider")}
              value={t("signInProviderValue")}
            />
            {roleLabel && (
              <DetailRow label={t("field.role")} value={roleLabel} />
            )}
          </div>
        </Card>

        {/* Sign Out */}
        <Card padding="lg">
          <Card.Title>{t("session.title")}</Card.Title>
          <Card.Description className="mt-1">
            {t("session.description")}
          </Card.Description>
          <form action="/auth/logout" method="POST" className="mt-4">
            <Button type="submit" variant="danger" size="md">
              {t("session.signOut")}
            </Button>
          </form>
        </Card>
      </div>
    </SettingsPageShell>
  );
}

function DetailRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-center justify-between px-6 py-3">
      <span className="text-sm text-foreground-muted">
        {label}
      </span>
      <span className="text-sm font-medium text-foreground-strong">
        {value}
      </span>
    </div>
  );
}

function roleHasTranslation(role: string): role is "admin" | "designer" | "viewer" {
  return role === "admin" || role === "designer" || role === "viewer";
}
