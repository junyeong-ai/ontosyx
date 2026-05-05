"use client";

// `useSelectionUrlSync` — round-trip the canvas selection through
// the URL so deep links carry it and the back button restores it.
//
// Encoding: `?sel=n:<id>,n:<id>,e:<id>,w:<id>` — kind shorthand
// (`n`=node, `e`=edge, `w`=widget) joined by colons, refs separated
// by commas. The order of the URL list is the *click order* — the
// last entry is the inspector primary, exactly like the in-memory
// model. An empty selection drops the param entirely so a pristine
// URL is, well, pristine.
//
// `router.replace` (not `push`) on every selection change so the
// back button hops between meaningful navigations, not every click.
// Initial mount reads the URL and seeds the store; subsequent
// changes flow store → URL.

import { useEffect, useRef } from "react";
import { usePathname, useRouter, useSearchParams } from "next/navigation";

import {
  refKey,
  useAppStore,
  type SelectionKind,
  type SelectionRef,
} from "@/lib/store";

const KIND_TO_SHORT: Record<SelectionKind, string> = {
  node: "n",
  edge: "e",
  widget: "w",
};
const SHORT_TO_KIND: Record<string, SelectionKind> = {
  n: "node",
  e: "edge",
  w: "widget",
};

export function encodeSelectionParam(refs: readonly SelectionRef[]): string {
  return refs.map((r) => `${KIND_TO_SHORT[r.kind]}:${r.id}`).join(",");
}

export function decodeSelectionParam(value: string | null): SelectionRef[] {
  if (!value) return [];
  const out: SelectionRef[] = [];
  const seen = new Set<string>();
  for (const part of value.split(",")) {
    const colon = part.indexOf(":");
    if (colon <= 0) continue;
    const short = part.slice(0, colon);
    const id = part.slice(colon + 1);
    const kind = SHORT_TO_KIND[short];
    if (!kind || !id) continue;
    const ref = { kind, id };
    const key = refKey(ref);
    if (seen.has(key)) continue;
    seen.add(key);
    out.push(ref);
  }
  return out;
}

export function useSelectionUrlSync(): void {
  const router = useRouter();
  const pathname = usePathname();
  const searchParams = useSearchParams();

  // `searchParams` is intentionally NOT a dep below — `router.replace`
  // mutates the URL, which mints a new `searchParams` reference on
  // every render. Wiring it into the deps creates a router.replace →
  // searchParams change → effect re-run → router.replace loop that
  // re-fetches the RSC payload forever. The latest-ref pattern keeps
  // the closure reading the freshest URL state without subscribing
  // to its identity changes.
  const searchParamsRef = useRef(searchParams);
  searchParamsRef.current = searchParams;

  // `lastEncoded` survives across effect re-runs so navigating away
  // and back doesn't replay the same `?sel=…` write — the previous
  // implementation declared it inline and reset it every time
  // `pathname` / `router` changed identity, defeating the dedup.
  const lastEncodedRef = useRef<string | null>(null);

  // Hydrate once: read the URL and apply it to the store, but only
  // when the URL actually has a value. A pristine page load with no
  // `sel` param should not stomp the persisted in-memory selection
  // (e.g. coming back to the workbench from another tab).
  const hydratedRef = useRef(false);
  useEffect(() => {
    if (hydratedRef.current) return;
    hydratedRef.current = true;
    const raw = searchParamsRef.current.get("sel");
    if (!raw) return;
    const refs = decodeSelectionParam(raw);
    if (refs.length > 0) {
      useAppStore.getState().selectMany(refs);
    }
    // searchParamsRef is intentionally read once on first mount via
    // the ref to avoid a re-hydration loop on every URL change.
  }, []);

  // Subscribe to selection changes and reflect them back into the URL.
  // The effect uses Zustand's `subscribe` directly so it runs once
  // per selection delta — re-rendering the host component would still
  // do the right thing, but the subscription form keeps the URL
  // writeback off the React render path.
  useEffect(() => {
    if (!hydratedRef.current) return;
    const apply = (encoded: string) => {
      if (encoded === lastEncodedRef.current) return;
      lastEncodedRef.current = encoded;
      const next = new URLSearchParams(searchParamsRef.current.toString());
      if (encoded) next.set("sel", encoded);
      else next.delete("sel");
      const search = next.toString();
      const url = `${pathname}${search ? `?${search}` : ""}`;
      router.replace(url, { scroll: false });
    };
    apply(encodeSelectionParam(useAppStore.getState().selection.refs));
    const unsubscribe = useAppStore.subscribe((state, prev) => {
      if (state.selection === prev.selection) return;
      apply(encodeSelectionParam(state.selection.refs));
    });
    return unsubscribe;
  }, [pathname, router]);
}
