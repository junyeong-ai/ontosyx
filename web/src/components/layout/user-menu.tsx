"use client";

import Link from "next/link";
import { useState } from "react";
import { useAuth } from "@/hooks/use-auth";
import { useTranslations } from "next-intl";
import { Popover, PopoverTrigger, PopoverContent } from "@/components/ui/popover";
import { Avatar } from "@/components/ui/avatar";
import { LocaleSwitcher } from "./locale-switcher";
import { clearCollabClient } from "@/lib/collab";

export function UserMenu() {
  const { mode } = useAuth();
  const [open, setOpen] = useState(false);
  const t = useTranslations("auth");

  // Render the menu in every signed-in mode — both `authenticated`
  // (multi-tenant) and `disabled` (dev / on-prem). The disabled path
  // omits the Sign Out form because there's no session to clear.
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
  const showSignOut = mode.kind === "authenticated";

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger className="flex shrink-0 items-center gap-2 rounded-md px-1.5 py-1 hover:bg-surface-inset focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand-foreground">
        <Avatar src={user.picture} name={user.name} size="xs" />
        <span className="max-w-[120px] truncate text-xs text-foreground-muted">
          {user.name}
        </span>
      </PopoverTrigger>
      <PopoverContent className="z-popover w-56 rounded-lg border border-divider bg-surface-base p-1 shadow-3 data-[starting-style]:scale-95 data-[starting-style]:opacity-0 data-[ending-style]:scale-95 data-[ending-style]:opacity-0 transition-all duration-[var(--duration-base)] ease-[var(--ease-out)]">
        <div className="px-3 py-2 text-xs text-foreground-muted">
          <div className="font-medium text-foreground-strong">
            {user.name}
          </div>
          <div className="mt-0.5 truncate">{user.email}</div>
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
        <LocaleSwitcher />
        {showSignOut && (
          <>
            <div className="my-1 h-px bg-surface-inset" />
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
          </>
        )}
      </PopoverContent>
    </Popover>
  );
}
