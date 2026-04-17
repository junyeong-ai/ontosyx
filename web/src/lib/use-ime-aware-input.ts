"use client";

import { useCallback, useRef, useState } from "react";

/**
 * IME-aware text input hook for Korean/Japanese/Chinese composition.
 *
 * Background: when a user types Hangul (한글), the browser fires multiple
 * `input`/`change` events for each jamo (자모) as the syllable composes.
 * A naive `onChange={(e) => setQuery(e.target.value)}` + `runSearch(query)`
 * flow ends up querying the API with intermediate jamo like "ㅎ", "하",
 * "한" — polluting results and firing unnecessary requests.
 *
 * Contract:
 * - The controlled `value` always mirrors the DOM (so the field displays
 *   mid-composition jamo as expected).
 * - `committedValue` only updates when composition ends (or for
 *   non-composition direct edits).
 * - Consumers run expensive side effects (fetch, debounce, filter) against
 *   `committedValue`, NOT `value`.
 *
 * Usage:
 * ```tsx
 * const input = useImeAwareInput("");
 * useEffect(() => { runSearch(input.committedValue); }, [input.committedValue]);
 * return <input value={input.value} {...input.bind} />;
 * ```
 */
export interface ImeAwareInputBinding {
  onChange: (e: React.ChangeEvent<HTMLInputElement | HTMLTextAreaElement>) => void;
  onCompositionStart: () => void;
  onCompositionEnd: (e: React.CompositionEvent<HTMLInputElement | HTMLTextAreaElement>) => void;
}

export interface ImeAwareInput {
  value: string;
  committedValue: string;
  setValue: (next: string) => void;
  bind: ImeAwareInputBinding;
}

export function useImeAwareInput(initial = ""): ImeAwareInput {
  const [value, setValue] = useState(initial);
  const [committedValue, setCommittedValue] = useState(initial);
  const composingRef = useRef(false);

  const onCompositionStart = useCallback(() => {
    composingRef.current = true;
  }, []);

  const onCompositionEnd = useCallback(
    (e: React.CompositionEvent<HTMLInputElement | HTMLTextAreaElement>) => {
      composingRef.current = false;
      const next = (e.target as HTMLInputElement | HTMLTextAreaElement).value;
      setValue(next);
      setCommittedValue(next);
    },
    [],
  );

  const onChange = useCallback(
    (e: React.ChangeEvent<HTMLInputElement | HTMLTextAreaElement>) => {
      const next = e.target.value;
      setValue(next);
      // Only commit when not mid-composition. For direct edits (Latin
      // characters, paste, backspace, etc.) the browser does not emit a
      // composition event pair, so we commit immediately.
      if (!composingRef.current) {
        setCommittedValue(next);
      }
    },
    [],
  );

  const setBoth = useCallback((next: string) => {
    setValue(next);
    setCommittedValue(next);
  }, []);

  return {
    value,
    committedValue,
    setValue: setBoth,
    bind: { onChange, onCompositionStart, onCompositionEnd },
  };
}
