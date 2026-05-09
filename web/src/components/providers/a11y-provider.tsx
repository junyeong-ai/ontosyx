"use client";

import { useEffect } from "react";

/**
 * A11yProvider — dev-only accessibility checker.
 *
 * Loads @axe-core/react once on mount and reports WCAG violations to the
 * browser console. Runs ONLY in development (NODE_ENV === "development");
 * production builds should never ship this runtime dependency.
 *
 * Wired into `app/layout.tsx` inside a dev-only branch so Next can still
 * tree-shake the axe bundle out of prod output.
 */
export function A11yProvider() {
  useEffect(() => {
    if (process.env.NODE_ENV !== "development") return;
    if (typeof window === "undefined") return;

    // Dynamically import so prod bundlers can drop this entirely.
    // Signature: axe(React, ReactDOM, delay, config)
    Promise.all([
      import("@axe-core/react"),
      import("react-dom"),
      import("react"),
    ])
      .then(([axe, ReactDOM, React]) => {
        axe.default(React.default, ReactDOM.default, 1000, undefined, {
          include: [["body"]],
          exclude: [
            ["nextjs-portal"],
            ["#nextjs-portal"],
            ["[data-nextjs-dialog-overlay]"],
          ],
        });
      })
      .catch((err) => {
        console.warn("[a11y] axe-core init failed:", err);
      });
  }, []);

  return null;
}
