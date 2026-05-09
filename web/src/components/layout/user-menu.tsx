"use client";

import Link from "next/link";
import { useState } from "react";
import { useAuth } from "@/hooks/use-auth";
import { useTranslations } from "next-intl";
import { Popover, PopoverTrigger, PopoverContent } from "@/components/ui/popover";
import { Avatar } from "@/components/ui/avatar";
import { LocaleSwitcher } from "./locale-switcher";
import { ThemeSwitcher } from "./theme-switcher";
import { clearCollabClient } from "@/lib/collab";

export function UserMenu() {
  const { mode } = useAuth();
  const [open, setOpen] = useState(false);
  const t = useTranslations("auth");

  if (mode.kind === "loading") return null;

  if (mode.kind === "unauthenticated") {
    return (
      <a
        href="/login"
        className="rounded-md border border-divider bg-surface-raised px-2.5 py-1 text-xs font-medium text-foreground hover:bg-surface-inset"
      >
        {t("signIn")}
      </a>
    );
  }

  const { user } = mode;
  const isDevMode = mode.kind === "disabled";

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger className="flex shrink-0 items-center gap-2 rounded-md px-1.5 py-1 hover:bg-surface-inset focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand-foreground">
        <Avatar src={user.picture} name={user.name} size="xs" />
        <span className="max-w-[120px] truncate text-xs text-foreground-muted">
          {user.name}
        </span>
      </PopoverTrigger>
      <PopoverContent className="popup-pop z-popover w-60 rounded-lg border border-divider bg-surface-base p-1 shadow-3">
        {/* Identity header — persona block sits above account
            actions so the menu opens with the operator's eye on
            "who am I logged in as". */}
        <div className="px-3 py-2 text-xs">
          <div className="font-medium text-foreground-strong">{user.name}</div>
          <div className="mt-0.5 truncate text-foreground-muted">{user.email}</div>
        </div>

        <div className="my-1 h-px bg-surface-inset" />

        <Link
          href="/account/profile"
          onClick={() => setOpen(false)}
          className="flex w-full items-center rounded-md px-3 py-1.5 text-start text-xs text-foreground hover:bg-surface-inset"
        >
          {t("myAccount")}
        </Link>

        <div className="my-1 h-px bg-surface-inset" />

        <ThemeSwitcher />
        <LocaleSwitcher />

        <div className="my-1 h-px bg-surface-inset" />

        {isDevMode ? (
          <div
            className="flex items-center gap-2 rounded-md px-3 py-1.5 text-2xs text-foreground-muted"
            role="status"
          >
            <span
              className="h-1.5 w-1.5 rounded-full bg-warning-foreground"
              aria-hidden="true"
            />
            <span className="font-medium uppercase tracking-wider">
              {t("devMode")}
            </span>
            <span className="ms-auto text-foreground-subtle">
              {t("devModeHint")}
            </span>
          </div>
        ) : (
          <form action="/auth/logout" method="POST">
            <button
              type="submit"
              className="flex w-full items-center rounded-md px-3 py-1.5 text-start text-xs text-danger-foreground hover:bg-danger-surface"
              onClick={() => {
                // Tear the collaboration WS down before the cookie
                // is wiped — once the form submits, the open socket
                // would otherwise keep streaming under a now-revoked
                // session for up to the periodic re-check window.
                clearCollabClient();
                setOpen(false);
              }}
            >
              {t("signOut")}
            </button>
          </form>
        )}
      </PopoverContent>
    </Popover>
  );
}
