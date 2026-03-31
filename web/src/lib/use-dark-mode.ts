"use client";

import { useEffect, useState } from "react";

/** Detect whether the user prefers dark mode */
export function useIsDarkMode(): boolean {
  const [dark, setDark] = useState(false);
  useEffect(() => {
    const mq = window.matchMedia("(prefers-color-scheme: dark)");
    setDark(mq.matches || document.documentElement.classList.contains("dark"));
    const handler = () =>
      setDark(mq.matches || document.documentElement.classList.contains("dark"));
    mq.addEventListener("change", handler);
    const observer = new MutationObserver(handler);
    observer.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ["class"],
    });
    return () => {
      mq.removeEventListener("change", handler);
      observer.disconnect();
    };
  }, []);
  return dark;
}
