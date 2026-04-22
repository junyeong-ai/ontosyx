"use client";

// Bootstrap wizard state persists in localStorage so a refresh or an
// accidental browser close doesn't lose the operator's progress.
// Keeping the schema tight — the wizard collects intent only; real
// writes (project creation, glossary edits, etc.) still go through
// the existing APIs once the user hits Finish on step 6.

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
} from "react";

const STORAGE_KEY = "ontosyx.bootstrap.v1";

export interface BootstrapState {
  pilotName: string;
  pilotScope: string;
  sourceKind: string;
  sourceConnection: string;
  glossaryDraft: string;
  rulesDraft: string;
  mappingNotes: string;
  completedSteps: string[];
}

const EMPTY: BootstrapState = {
  pilotName: "",
  pilotScope: "",
  sourceKind: "",
  sourceConnection: "",
  glossaryDraft: "",
  rulesDraft: "",
  mappingNotes: "",
  completedSteps: [],
};

interface Ctx {
  state: BootstrapState;
  update: (patch: Partial<BootstrapState>) => void;
  markComplete: (stepKey: string) => void;
  reset: () => void;
}

const BootstrapCtx = createContext<Ctx | null>(null);

export function BootstrapProvider({ children }: { children: React.ReactNode }) {
  const [state, setState] = useState<BootstrapState>(EMPTY);
  const [hydrated, setHydrated] = useState(false);

  useEffect(() => {
    try {
      const raw = window.localStorage.getItem(STORAGE_KEY);
      if (raw) {
        const parsed = JSON.parse(raw) as Partial<BootstrapState>;
        setState({ ...EMPTY, ...parsed });
      }
    } catch {
      // Corrupt payload — start clean, don't block the wizard.
    }
    setHydrated(true);
  }, []);

  useEffect(() => {
    if (!hydrated) return;
    try {
      window.localStorage.setItem(STORAGE_KEY, JSON.stringify(state));
    } catch {
      // Quota or disabled — in-memory is still functional.
    }
  }, [state, hydrated]);

  const update = useCallback((patch: Partial<BootstrapState>) => {
    setState((s) => ({ ...s, ...patch }));
  }, []);

  const markComplete = useCallback((stepKey: string) => {
    setState((s) =>
      s.completedSteps.includes(stepKey)
        ? s
        : { ...s, completedSteps: [...s.completedSteps, stepKey] },
    );
  }, []);

  const reset = useCallback(() => {
    setState(EMPTY);
    try {
      window.localStorage.removeItem(STORAGE_KEY);
    } catch {
      // no-op
    }
  }, []);

  const value = useMemo(
    () => ({ state, update, markComplete, reset }),
    [state, update, markComplete, reset],
  );

  return <BootstrapCtx.Provider value={value}>{children}</BootstrapCtx.Provider>;
}

export function useBootstrap() {
  const ctx = useContext(BootstrapCtx);
  if (!ctx) {
    throw new Error("useBootstrap must be called inside <BootstrapProvider>");
  }
  return ctx;
}
