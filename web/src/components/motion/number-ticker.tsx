"use client";

import { useEffect, useRef, useState } from "react";

interface NumberTickerProps {
  value: number;
  /** Total animation duration in ms. */
  durationMs?: number;
  /** Number of decimals to render. */
  decimals?: number;
  /** Optional formatter applied to the rendered number. */
  format?: (n: number) => string;
  className?: string;
}

const easeOutCubic = (t: number) => 1 - Math.pow(1 - t, 3);

export function NumberTicker({
  value,
  durationMs = 600,
  decimals = 0,
  format,
  className,
}: NumberTickerProps) {
  const [display, setDisplay] = useState(value);
  const fromRef = useRef(value);
  const startRef = useRef<number | null>(null);
  const rafRef = useRef<number | null>(null);

  useEffect(() => {
    const prefersReduced =
      typeof window !== "undefined" &&
      window.matchMedia?.("(prefers-reduced-motion: reduce)").matches;

    if (prefersReduced || durationMs <= 0) {
      setDisplay(value);
      fromRef.current = value;
      return;
    }

    fromRef.current = display;
    startRef.current = null;

    const tick = (now: number) => {
      if (startRef.current === null) startRef.current = now;
      const elapsed = now - startRef.current;
      const t = Math.min(elapsed / durationMs, 1);
      const eased = easeOutCubic(t);
      const next = fromRef.current + (value - fromRef.current) * eased;
      setDisplay(next);

      if (t < 1) {
        rafRef.current = requestAnimationFrame(tick);
      } else {
        setDisplay(value);
      }
    };

    rafRef.current = requestAnimationFrame(tick);
    return () => {
      if (rafRef.current !== null) cancelAnimationFrame(rafRef.current);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [value, durationMs]);

  const rendered = format
    ? format(display)
    : decimals === 0
      ? Math.round(display).toLocaleString()
      : display.toFixed(decimals);

  return <span className={className}>{rendered}</span>;
}
