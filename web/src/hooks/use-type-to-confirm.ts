"use client";

// `useTypeToConfirm` — state companion to `<TypeToConfirmField>`.
//
// The field is lifted-state; the hook owns the typed value, the
// match predicate, and the reset-on-phrase-change behaviour. A
// destructive form pulls the hook, threads `value` / `onChange`
// into the field, and gates the submit button on `matches`.
//
// Why a hook instead of internal field state: the parent button's
// disabled gate depends on the match. If the field owned its own
// state, the parent would have to either lift it or duplicate the
// match predicate. Lifting wins — one source of truth, one place
// to reset.

import { useCallback, useState } from "react";

import { matchesConfirmPhrase } from "@/components/ui/type-to-confirm";

export interface TypeToConfirmState {
  value: string;
  onChange: (next: string) => void;
  matches: boolean;
  /** Reset the typed value to empty. Useful after a successful submit. */
  reset: () => void;
}

interface InternalState {
  /** The phrase the typed value was last validated against. */
  trackedPhrase: string;
  value: string;
}

export function useTypeToConfirm(phrase: string): TypeToConfirmState {
  // Derive-on-render: keep the typed value alongside the phrase it
  // was last bound to. When the phrase changes (re-opening a confirm
  // flow against a different resource) the value is silently
  // dropped — re-using stale typed text against a new phrase would
  // be confusing and, because `matches` would silently re-evaluate,
  // potentially dangerous. The conditional setState during render
  // is the React-19-blessed alternative to a setState-in-effect.
  const [state, setState] = useState<InternalState>(() => ({
    trackedPhrase: phrase,
    value: "",
  }));
  if (state.trackedPhrase !== phrase) {
    setState({ trackedPhrase: phrase, value: "" });
  }

  const onChange = useCallback(
    (next: string) =>
      setState((prev) => ({ trackedPhrase: prev.trackedPhrase, value: next })),
    [],
  );
  const reset = useCallback(
    () =>
      setState((prev) => ({ trackedPhrase: prev.trackedPhrase, value: "" })),
    [],
  );

  return {
    value: state.value,
    onChange,
    reset,
    matches: matchesConfirmPhrase(state.value, phrase),
  };
}
