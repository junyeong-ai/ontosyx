"use client";

// `<ExtensionsBoot />` — tiny client component that runs the
// `installExtensions(EXTENSIONS)` boot pass once on the client.
//
// Why a component instead of a top-level side-effect import: the
// registries are module singletons that are read by client-only code
// (the sidebar, the inspector). Doing the install at module-load on
// the server (e.g. from layout.tsx) would mutate state during SSR
// that the hydration pass then has to match. A client-only boot
// component keeps the install confined to the same lifecycle the
// consumers run in.
//
// The boot runs on first mount; subsequent re-mounts (HMR, fast
// refresh, route transitions that re-render the layout) re-invoke
// the install pass which is idempotent — see
// `installExtensions()` for the dedup + uninstall semantics.

import { useEffect } from "react";

import { EXTENSIONS, installExtensions } from "./index";

export function ExtensionsBoot() {
  useEffect(() => {
    installExtensions(EXTENSIONS);
  }, []);
  return null;
}
