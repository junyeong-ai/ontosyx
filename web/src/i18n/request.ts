// ---------------------------------------------------------------------------
// next-intl request configuration
// ---------------------------------------------------------------------------
//
// Cookie-based locale resolution — the platform is Korean-first, with English
// as an opt-in switch (`ontosyx_locale=en`). Putting the locale in the URL
// would fight the existing Zustand-driven `workspaceMode` router; we keep
// URLs stable and flip the locale via cookie. Phase 2-4 revisits this once
// dynamic segments (`[locale]`) are introduced.
//
// Fallback order:
//   1. Explicit `ontosyx_locale` cookie (set by the locale switcher in the
//      user menu — see `Phase 2-3` follow-up).
//   2. `Accept-Language` header — defer until we actually need browser
//      auto-detection; cookie-first is predictable enough for now.
//   3. Built-in default: `ko`.
//
// The `SUPPORTED_LOCALES` list gates user-supplied cookie values so a
// tampered cookie cannot load an arbitrary module path.
// ---------------------------------------------------------------------------

import { cookies } from "next/headers";
import { getRequestConfig } from "next-intl/server";

import {
  SUPPORTED_LOCALES,
  DEFAULT_LOCALE,
  DEFAULT_TIME_ZONE,
  type Locale,
} from "./config";

export default getRequestConfig(async () => {
  const store = await cookies();
  const raw = store.get("ontosyx_locale")?.value;
  const locale: Locale =
    raw && (SUPPORTED_LOCALES as readonly string[]).includes(raw)
      ? (raw as Locale)
      : DEFAULT_LOCALE;

  return {
    locale,
    timeZone: DEFAULT_TIME_ZONE,
    messages: (await import(`../../messages/${locale}.json`)).default,
  };
});
