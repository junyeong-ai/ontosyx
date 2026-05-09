import type { StateCreator } from "zustand";
import type {
  AppStore,
  NotificationCount,
  NotificationSlice,
} from "./types";

export const createNotificationSlice: StateCreator<
  AppStore,
  [],
  [],
  NotificationSlice
> = (set) => ({
  modeCounts: {},
  publishModeCount: (modeId, count) => {
    set((state) => {
      const prev = state.modeCounts[modeId];
      if (
        prev &&
        prev.count === count.count &&
        prev.tone === count.tone
      ) {
        return state;
      }
      if (count.count <= 0) {
        if (!prev) return state;
        const next = { ...state.modeCounts };
        delete next[modeId];
        return { modeCounts: next };
      }
      return {
        modeCounts: { ...state.modeCounts, [modeId]: count },
      };
    });
  },
  clearModeCount: (modeId) => {
    set((state) => {
      if (!state.modeCounts[modeId]) return state;
      const next = { ...state.modeCounts };
      delete next[modeId];
      return { modeCounts: next };
    });
  },
  clearAllModeCounts: () => {
    set((state) =>
      Object.keys(state.modeCounts).length === 0
        ? state
        : { modeCounts: {} },
    );
  },
});

export type { NotificationCount };
