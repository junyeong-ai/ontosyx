"use client";

import { AnimatePresence, motion } from "motion/react";
import type { ReactNode } from "react";

interface PageTransitionProps {
  /** Stable identifier — when this changes the previous tree exits and
   *  the new one enters. Pass the route pathname for route-driven
   *  transitions. */
  motionKey: string;
  children: ReactNode;
}

/**
 * Symmetric route transition. `AnimatePresence mode="wait"` is baked in
 * so the previous tree finishes its exit before the next mounts — every
 * caller gets a complete mount + unmount animation without remembering
 * to wrap it themselves. Without this internal wrapper the `exit` prop
 * is silently dropped and the user sees an instant disappear / fade-in.
 */
export function PageTransition({ motionKey, children }: PageTransitionProps) {
  return (
    <AnimatePresence mode="wait" initial={false}>
      <motion.div
        key={motionKey}
        initial={{ opacity: 0, y: 4 }}
        animate={{ opacity: 1, y: 0 }}
        exit={{ opacity: 0, y: -4 }}
        transition={{ duration: 0.2, ease: [0.22, 1, 0.36, 1] }}
        className="h-full"
      >
        {children}
      </motion.div>
    </AnimatePresence>
  );
}
