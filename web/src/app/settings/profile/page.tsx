"use client";

import { useTranslations } from "next-intl";
import { useAuth } from "@/lib/use-auth";
import { Spinner } from "@/components/ui/spinner";
import { Avatar } from "@/components/ui/avatar";

const ROLE_COLORS: Record<string, string> = {
  admin:
    "bg-indigo-100 text-indigo-700 dark:bg-indigo-900/30 dark:text-indigo-400",
  designer:
    "bg-emerald-100 text-emerald-700 dark:bg-emerald-900/30 dark:text-emerald-400",
  viewer:
    "bg-zinc-100 text-zinc-600 dark:bg-zinc-800 dark:text-muted-foreground",
};

export default function ProfilePage() {
  const t = useTranslations("settings.profile");
  const roleT = useTranslations("settings.roles");
  const authT = useTranslations("auth");
  const { user, loading, authEnabled } = useAuth();

  if (loading) {
    return (
      <div className="flex items-center justify-center py-20">
        <Spinner size="lg" className="text-emerald-500" />
      </div>
    );
  }

  // Dev mode — no auth configured
  if (!authEnabled) {
    return (
      <div>
        <h1 className="text-xl font-semibold text-zinc-900 dark:text-zinc-100">
          {t("title")}
        </h1>
        <div className="mt-6 rounded-lg border border-zinc-200 bg-white p-6 dark:border-zinc-800 dark:bg-zinc-900">
          <div className="flex items-center gap-3">
            <div className="flex h-12 w-12 items-center justify-center rounded-full bg-zinc-200 text-lg font-semibold text-zinc-500 dark:bg-zinc-700 dark:text-muted-foreground">
              D
            </div>
            <div>
              <p className="text-sm font-medium text-zinc-900 dark:text-zinc-100">
                {t("devMode.userName")}
              </p>
              <p className="text-xs text-zinc-500 dark:text-muted-foreground">
                {t("devMode.userEmail")}
              </p>
            </div>
          </div>
          <div className="mt-4 rounded-md border border-amber-200 bg-amber-50 px-4 py-3 text-sm text-amber-800 dark:border-amber-800 dark:bg-amber-950/30 dark:text-amber-300">
            {t("devMode.notice")}
          </div>
        </div>
      </div>
    );
  }

  if (!user) {
    return (
      <div>
        <h1 className="text-xl font-semibold text-zinc-900 dark:text-zinc-100">
          {t("title")}
        </h1>
        <div className="mt-6 rounded-lg border border-zinc-200 bg-white p-6 text-center dark:border-zinc-800 dark:bg-zinc-900">
          <p className="text-sm text-zinc-500 dark:text-muted-foreground">
            {t("notSignedIn")}
          </p>
          <a
            href="/login"
            className="mt-3 inline-block rounded-md bg-emerald-600 px-4 py-2 text-sm font-medium text-white hover:bg-emerald-700"
          >
            {authT("signIn")}
          </a>
        </div>
      </div>
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
      "bg-zinc-100 text-zinc-600 dark:bg-zinc-800 dark:text-muted-foreground"
    : undefined;

  return (
    <div>
      <h1 className="text-xl font-semibold text-zinc-900 dark:text-zinc-100">
        {t("title")}
      </h1>

      <div className="mt-6 space-y-6">
        {/* Avatar & Identity */}
        <section className="rounded-lg border border-zinc-200 bg-white p-6 dark:border-zinc-800 dark:bg-zinc-900">
          <div className="flex items-center gap-4">
            <Avatar src={user.picture} name={user.name} size="lg" />
            <div>
              <p className="text-lg font-semibold text-zinc-900 dark:text-zinc-100">
                {user.name}
              </p>
              <p className="text-sm text-zinc-500 dark:text-muted-foreground">
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
        </section>

        {/* Account Details */}
        <section className="rounded-lg border border-zinc-200 bg-white dark:border-zinc-800 dark:bg-zinc-900">
          <div className="border-b border-zinc-100 px-6 py-4 dark:border-zinc-800">
            <h2 className="text-sm font-semibold text-zinc-900 dark:text-zinc-100">
              {t("accountDetails")}
            </h2>
          </div>
          <div className="divide-y divide-zinc-100 dark:divide-zinc-800">
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
        </section>

        {/* Sign Out */}
        <section className="rounded-lg border border-zinc-200 bg-white p-6 dark:border-zinc-800 dark:bg-zinc-900">
          <h2 className="text-sm font-semibold text-zinc-900 dark:text-zinc-100">
            {t("session.title")}
          </h2>
          <p className="mt-1 text-xs text-zinc-500 dark:text-muted-foreground">
            {t("session.description")}
          </p>
          <form action="/auth/logout" method="POST" className="mt-4">
            <button
              type="submit"
              className="rounded-md border border-red-200 bg-white px-4 py-2 text-sm font-medium text-red-600 transition-colors hover:bg-red-50 dark:border-red-800 dark:bg-zinc-900 dark:text-red-400 dark:hover:bg-red-950/30"
            >
              {t("session.signOut")}
            </button>
          </form>
        </section>
      </div>
    </div>
  );
}

function DetailRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-center justify-between px-6 py-3">
      <span className="text-sm text-zinc-500 dark:text-muted-foreground">
        {label}
      </span>
      <span className="text-sm font-medium text-zinc-900 dark:text-zinc-100">
        {value}
      </span>
    </div>
  );
}

function roleHasTranslation(role: string): role is "admin" | "designer" | "viewer" {
  return role === "admin" || role === "designer" || role === "viewer";
}
