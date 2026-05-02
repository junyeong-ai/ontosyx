/**
 * Level-based browser logger.
 *
 * Use this module instead of calling `console.*` directly. The `no-console`
 * ESLint rule (configured in `eslint.config.mjs`) blocks ad-hoc `console.log`
 * / `console.info` / `console.debug` across the codebase; `console.warn` and
 * `console.error` remain allowed for exceptional cases (e.g. Zod validation
 * warnings or uncaught error boundaries).
 *
 * Levels:
 *  - `debug`: gated on `process.env.NEXT_PUBLIC_LOG_LEVEL === "debug"`
 *  - `info`: always emits
 *  - `warn`: always emits (calls the allowed `console.warn`)
 *  - `error`: always emits (calls the allowed `console.error`)
 *
 * The `console.info` and `console.debug` calls below are intentionally the
 * only info/debug console calls in the codebase; the `eslint-disable-next-line`
 * comments localize the exemption to this file alone.
 */
// `console.{debug,info}` calls below are intentional — this file is
// the project's logging primitive; every other call site is forbidden
// from `console` and routes through these methods.
export const logger = {
  debug: (...args: unknown[]) => {
    if (process.env.NEXT_PUBLIC_LOG_LEVEL === "debug") {
      // eslint-disable-next-line no-console
      console.debug("[ox]", ...args);
    }
  },
  info: (...args: unknown[]) => {
    // eslint-disable-next-line no-console
    console.info("[ox]", ...args);
  },
  warn: (...args: unknown[]) => console.warn("[ox]", ...args),
  error: (...args: unknown[]) => console.error("[ox]", ...args),
};
