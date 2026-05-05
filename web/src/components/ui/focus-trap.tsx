"use client";

// `<FocusTrap>` — lazy-loaded façade around `focus-trap-react`.
//
// `focus-trap-react` (and its tabbable / focus-trap dependencies)
// total ~12kb gzipped. Three modal surfaces in the workbench reach
// for it (keyboard-shortcuts dialog, global command palette, search
// dialog), and a user touches none of them on initial app paint. The
// implementation is module-cached: the first modal mount triggers
// the dynamic import and stashes the resolved component at module
// scope; every subsequent mount in the same session reads the cache
// synchronously and renders the trap on the first frame, so the
// "children render unwrapped while chunk loads" window only ever
// happens once per session.
//
// `ssr: false` semantics are achieved naturally — the dynamic
// import is kicked off from `useEffect`, which never runs on the
// server. SSR renders the `children` unwrapped (the safe fallback);
// the trap mounts on the client after hydration.

import { useEffect, useState, type ComponentType } from "react";
import type { ComponentProps } from "react";
import type FocusTrapImpl from "focus-trap-react";

type FocusTrapProps = ComponentProps<typeof FocusTrapImpl>;
type FocusTrapComponent = ComponentType<FocusTrapProps>;

let cachedImpl: FocusTrapComponent | null = null;
let pendingImport: Promise<FocusTrapComponent> | null = null;

function loadImpl(): Promise<FocusTrapComponent> {
  if (cachedImpl) return Promise.resolve(cachedImpl);
  if (!pendingImport) {
    pendingImport = import("focus-trap-react").then((m) => {
      const impl = m.default;
      cachedImpl = impl;
      return impl;
    });
  }
  return pendingImport;
}

export function FocusTrap({ children, ...rest }: FocusTrapProps) {
  // Seed from the cache so the second-and-onward mount renders the
  // trap synchronously without a setState round-trip.
  const [Impl, setImpl] = useState<FocusTrapComponent | null>(cachedImpl);
  useEffect(() => {
    if (Impl) return;
    let cancelled = false;
    void loadImpl().then((next) => {
      if (!cancelled) setImpl(() => next);
    });
    return () => {
      cancelled = true;
    };
  }, [Impl]);
  if (!Impl) return <>{children}</>;
  return <Impl {...rest}>{children}</Impl>;
}
