"use client";

import { motion } from "motion/react";
import { Children, type ReactNode, isValidElement, cloneElement } from "react";

interface StaggerListProps {
  /** Delay between consecutive child enter animations. */
  stagger?: number;
  /** Initial delay before the first child enters. */
  initialDelay?: number;
  /** Skip animation entirely once the list grows beyond this many items
   *  — perf protection for long virtualised lists. */
  maxItems?: number;
  children: ReactNode;
  className?: string;
}

const itemVariants = {
  hidden: { opacity: 0, y: 6 },
  visible: { opacity: 1, y: 0 },
};

export function StaggerList({
  stagger = 0.04,
  initialDelay = 0.05,
  maxItems = 12,
  children,
  className,
}: StaggerListProps) {
  const items = Children.toArray(children);
  if (items.length > maxItems) {
    return <div className={className}>{children}</div>;
  }

  return (
    <motion.div
      className={className}
      initial="hidden"
      animate="visible"
      transition={{
        staggerChildren: stagger,
        delayChildren: initialDelay,
      }}
    >
      {items.map((child, i) => {
        const key =
          isValidElement(child) && child.key !== null ? child.key : i;
        return (
          <motion.div
            key={key}
            variants={itemVariants}
            transition={{ duration: 0.2, ease: [0.22, 1, 0.36, 1] }}
          >
            {isValidElement(child) ? cloneElement(child) : child}
          </motion.div>
        );
      })}
    </motion.div>
  );
}
