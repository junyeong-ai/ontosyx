"use client";

import { useState } from "react";
import { useAuth } from "@/lib/use-auth";
import { useTranslations } from "next-intl";
import { Popover, PopoverTrigger, PopoverContent } from "@/components/ui/popover";
import { Avatar } from "@/components/ui/avatar";
import { LocaleSwitcher } from "./locale-switcher";

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
        className="rounded-md border border-zinc-200 bg-zinc-50 px-2.5 py-1 text-xs font-medium text-zinc-700 hover:bg-zinc-100 dark:border-zinc-700 dark:bg-zinc-900 dark:text-zinc-300 dark:hover:bg-zinc-800"
      >
        {t("signIn")}
      </a>
    );
  }

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger className="flex shrink-0 items-center gap-2 rounded-md px-1.5 py-1 hover:bg-zinc-100 dark:hover:bg-zinc-800">
        <Avatar src={user.picture} name={user.name} size="xs" />
        <span className="max-w-[120px] truncate text-xs text-zinc-600 dark:text-muted-foreground">
          {user.name}
        </span>
      </PopoverTrigger>
      <PopoverContent className="z-50 w-56 rounded-lg border border-zinc-200 bg-white p-1 shadow-lg data-[starting-style]:scale-95 data-[starting-style]:opacity-0 data-[ending-style]:scale-95 data-[ending-style]:opacity-0 transition-all dark:border-zinc-700 dark:bg-zinc-900">
        <div className="px-3 py-2 text-xs text-zinc-500 dark:text-muted-foreground">
          <div className="font-medium text-zinc-700 dark:text-zinc-200">
            {user.name}
          </div>
          <div className="mt-0.5 truncate">{user.email}</div>
        </div>
        <div className="my-1 h-px bg-zinc-200 dark:bg-zinc-700" />
        <LocaleSwitcher />
        <div className="my-1 h-px bg-zinc-200 dark:bg-zinc-700" />
        <form action="/auth/logout" method="POST">
          <button
            type="submit"
            className="flex w-full items-center rounded-md px-3 py-1.5 text-left text-xs text-red-600 hover:bg-red-50 dark:text-red-400 dark:hover:bg-red-950"
            onClick={() => setOpen(false)}
          >
            {t("signOut")}
          </button>
        </form>
      </PopoverContent>
    </Popover>
  );
}
