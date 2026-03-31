"use client";

import { useState } from "react";
import { useAuth } from "@/lib/use-auth";
import { Popover, PopoverTrigger, PopoverContent } from "@/components/ui/popover";

export function UserMenu() {
  const { user, loading, authEnabled } = useAuth();
  const [open, setOpen] = useState(false);

  // Don't render anything while loading or in dev mode
  if (loading || !authEnabled) return null;

  if (!user) {
    return (
      <a
        href="/login"
        className="rounded-md border border-zinc-200 bg-zinc-50 px-2.5 py-1 text-xs font-medium text-zinc-700 hover:bg-zinc-100 dark:border-zinc-700 dark:bg-zinc-900 dark:text-zinc-300 dark:hover:bg-zinc-800"
      >
        Sign in
      </a>
    );
  }

  const initials = user.name
    .split(" ")
    .map((n) => n[0])
    .join("")
    .toUpperCase()
    .slice(0, 2);

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger className="flex shrink-0 items-center gap-2 rounded-md px-1.5 py-1 hover:bg-zinc-100 dark:hover:bg-zinc-800">
        {user.picture ? (
          <img
            src={user.picture}
            alt={user.name}
            className="h-6 w-6 shrink-0 rounded-full"
            referrerPolicy="no-referrer"
          />
        ) : (
          <div className="flex h-6 w-6 shrink-0 items-center justify-center rounded-full bg-indigo-600 text-[10px] font-semibold text-white">
            {initials}
          </div>
        )}
        <span className="max-w-[120px] truncate text-xs text-zinc-600 dark:text-zinc-400">
          {user.name}
        </span>
      </PopoverTrigger>
      <PopoverContent className="z-50 w-56 rounded-lg border border-zinc-200 bg-white p-1 shadow-lg data-[starting-style]:scale-95 data-[starting-style]:opacity-0 data-[ending-style]:scale-95 data-[ending-style]:opacity-0 transition-all dark:border-zinc-700 dark:bg-zinc-900">
        <div className="px-3 py-2 text-xs text-zinc-500 dark:text-zinc-400">
          <div className="font-medium text-zinc-700 dark:text-zinc-200">
            {user.name}
          </div>
          <div className="mt-0.5 truncate">{user.email}</div>
        </div>
        <div className="my-1 h-px bg-zinc-200 dark:bg-zinc-700" />
        <form action="/auth/logout" method="POST">
          <button
            type="submit"
            className="flex w-full items-center rounded-md px-3 py-1.5 text-left text-xs text-red-600 hover:bg-red-50 dark:text-red-400 dark:hover:bg-red-950"
            onClick={() => setOpen(false)}
          >
            Sign out
          </button>
        </form>
      </PopoverContent>
    </Popover>
  );
}
