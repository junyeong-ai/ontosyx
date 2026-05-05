// Toast duration constants — single source of truth for the four
// archetypal toast persistence windows. Keep these stable; if a
// surface needs a longer window because it carries a long URL or
// remediation steps, prefer `WARNING` (longer) over inventing a
// per-surface number.
//
// The numbers are tuned for readers who are scanning, not anyone
// hovering. The toast component pauses the timer on hover, so a
// reader who needs the full text in front of them gets it without
// the page racing ahead.

/** Brief acknowledgement — "saved", "copied", "cleared". */
export const TOAST_SUCCESS_FAST = 2_500;

/** Default success / info banner — "workspace switched", "model
 *  config updated". */
export const TOAST_INFO = 4_000;

/** Warnings the reader needs longer to absorb — partial state
 *  changes, configuration drift, deprecation notices. */
export const TOAST_WARNING = 5_000;

/** Errors that may require remediation — failed save, network
 *  trouble. Long enough to read the message + glance at any URL or
 *  remediation hint, short enough to dismiss naturally. */
export const TOAST_ERROR = 6_000;
