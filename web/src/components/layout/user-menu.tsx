"use client";

import { useState } from "react";
import { useAuth } from "@/hooks/use-auth";
import { useTranslations } from "next-intl";
import { Popover, PopoverTrigger, PopoverContent } from "@/components/ui/popover";
import { Avatar } from "@/components/ui/avatar";
import { LocaleSwitcher } from "./locale-switcher";
import { clearCollabClient } from "@/lib/collab";

export function UserMenu() {
  const { user, loading, authEnabled } = useAuth();
  const [open, setOpen] = useState(false);
  const t = useTranslations("auth");

  // Don't render anything while loading or in dev mode
  if (loading || !authEnabled) return null;

  if (!user) {
    return (
      <a
        href="/login"
        className="rounded-md border border-divider bg-surface-raised px-2.5 py-1 text-xs font-medium text-foreground hover:bg-surface-inset-muted dark:hover:bg-zinc-800"
      >
        {t("signIn")}
      </a>
    );
  }

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger className="flex shrink-0 items-center gap-2 rounded-md px-1.5 py-1 hover:bg-surface-inset dark:hover:bg-zinc-800">
        <Avatar src={user.picture} name={user.name} size="xs" />
        <span className="max-w-[120px] truncate text-xs text-foreground dark:text-muted-foreground">
          {user.name}
        </span>
      </PopoverTrigger>
      <PopoverContent className="z-50 w-56 rounded-lg border border-divider bg-surface-base p-1 shadow-lg data-[starting-style]:scale-95 data-[starting-style]:opacity-0 data-[ending-style]:scale-95 data-[ending-style]:opacity-0 transition-all">
        <div className="px-3 py-2 text-xs text-foreground-muted">
          <div className="font-medium text-foreground-strong">
            {user.name}
          </div>
          <div className="mt-0.5 truncate">{user.email}</div>
        </div>
        <div className="my-1 h-px bg-surface-inset" />
        <LocaleSwitcher />
        <div className="my-1 h-px bg-surface-inset" />
        <form action="/auth/logout" method="POST">
          <button
            type="submit"
            className="flex w-full items-center rounded-md px-3 py-1.5 text-left text-xs text-danger-foreground hover:bg-danger-surface dark:hover:bg-danger-surface"
            onClick={() => {
              // Tear the collaboration WS down before the cookie
              // is wiped — once the form submits the token revoke
              // round-trip, the open socket would otherwise keep
              // streaming under a now-revoked session for up to
              // the periodic re-check window.
              clearCollabClient();
              setOpen(false);
            }}
          >
            {t("signOut")}
          </button>
        </form>
      </PopoverContent>
    </Popover>
  );
}
