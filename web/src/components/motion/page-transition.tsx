"use client";

import { motion } from "motion/react";
import type { ReactNode } from "react";

interface PageTransitionProps {
  /** Stable identifier — when this changes the transition replays.
   *  Pass the route pathname for route-driven transitions. */
  motionKey: string;
  children: ReactNode;
}

export function PageTransition({ motionKey, children }: PageTransitionProps) {
  return (
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
  );
}
