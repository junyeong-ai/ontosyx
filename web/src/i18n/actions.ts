"use server";

import { cookies } from "next/headers";
import { revalidatePath } from "next/cache";

import {
  DEFAULT_LOCALE,
  LOCALE_COOKIE,
  SUPPORTED_LOCALES,
  type Locale,
} from "./config";

/**
 * Persist the user's locale choice to a cookie and re-render the whole
 * tree under the new locale. Called from client components via a form
 * action or `startTransition` — see `locale-switcher.tsx`.
 *
 * The cookie is HttpOnly-false (readable from JS) because the
 * client-side next-intl provider reads it back on the next navigation;
 * gating on server-only would force a round-trip for every render.
 * Contents are a whitelisted enum so script-setting a bogus value
 * falls through to the default.
 */
export async function setLocaleAction(next: string): Promise<void> {
  const locale: Locale = (SUPPORTED_LOCALES as readonly string[]).includes(next)
    ? (next as Locale)
    : DEFAULT_LOCALE;

  const store = await cookies();
  store.set(LOCALE_COOKIE, locale, {
    path: "/",
    // 1 year — locale is a user preference, not a session state
    maxAge: 60 * 60 * 24 * 365,
    sameSite: "lax",
  });

  // Force the whole App Router tree to re-render under the new locale.
  // `revalidatePath("/", "layout")` invalidates the root layout cache
  // so `getLocale()` and `getMessages()` re-run on the next request.
  revalidatePath("/", "layout");
}
