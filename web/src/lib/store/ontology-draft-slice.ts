import type { StateCreator } from "zustand";
import type { AppStore, OntologyDraftSlice } from "./types";

export const createOntologyDraftSlice: StateCreator<AppStore, [], [], OntologyDraftSlice> = (set) => ({
  activeOntologyDraft: null,
  setActiveOntologyDraft: (project) => set({ activeOntologyDraft: project }),

  lastReconcileReport: null,
  setLastReconcileReport: (report) => set({ lastReconcileReport: report }),

  pendingReconcile: null,
  setPendingReconcile: (reconcile) => set({ pendingReconcile: reconcile }),

  activeDiffOverlay: null,
  setActiveDiffOverlay: (diff) => set({ activeDiffOverlay: diff }),
});
