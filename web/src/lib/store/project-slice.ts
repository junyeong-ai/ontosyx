import type { StateCreator } from "zustand";
import type { AppStore, ProjectSlice } from "./types";

export const createProjectSlice: StateCreator<AppStore, [], [], ProjectSlice> = (set) => ({
  activeProject: null,
  setActiveProject: (project) => set({ activeProject: project }),

  lastReconcileReport: null,
  setLastReconcileReport: (report) => set({ lastReconcileReport: report }),

  pendingReconcile: null,
  setPendingReconcile: (reconcile) => set({ pendingReconcile: reconcile }),

  activeDiffOverlay: null,
  setActiveDiffOverlay: (diff) => set({ activeDiffOverlay: diff }),
});
