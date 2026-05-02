"use client";

import { motion } from "motion/react";
import type { ReactNode } from "react";

interface FadeInProps {
  delay?: number;
  duration?: number;
  children: ReactNode;
  className?: string;
}

export function FadeIn({
  delay = 0,
  duration = 0.2,
  children,
  className,
}: FadeInProps) {
  return (
    <motion.div
      className={className}
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      transition={{ duration, delay, ease: [0.22, 1, 0.36, 1] }}
    >
      {children}
    </motion.div>
  );
}
