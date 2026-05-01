// next-intl request configuration.
//
// Cookie-based locale resolution — Korean-first with English opt-in
// via `ontosyx_locale=en`. Fallback order:
//   1. `ontosyx_locale` cookie set by the locale switcher.
//   2. `Accept-Language` header (deferred until needed).
//   3. Default `ko`.
//
// `SUPPORTED_LOCALES` gates user-supplied cookie values so a tampered
// cookie cannot load an arbitrary module path.

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
