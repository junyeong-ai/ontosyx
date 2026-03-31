import type { StateCreator } from "zustand";
import type { AppStore, SelectionSlice } from "./types";

export const createSelectionSlice: StateCreator<AppStore, [], [], SelectionSlice> = (set) => ({
  selection: { type: "none" },
  select: (selection) => set({ selection }),
  clearSelection: () => set({ selection: { type: "none" } }),
  neighborhoodFocus: null,
  setNeighborhoodFocus: (focus) => set({ neighborhoodFocus: focus }),
});
