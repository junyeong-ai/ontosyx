import type { StateCreator } from "zustand";
import type { AppStore, DashboardSlice } from "./types";

export const createDashboardSlice: StateCreator<AppStore, [], [], DashboardSlice> = (set) => ({
  activeDashboardId: null,
  setActiveDashboardId: (id) => set({ activeDashboardId: id }),
  dashboardWidgetCount: 0,
  setDashboardWidgetCount: (count) => set({ dashboardWidgetCount: count }),
  dashboardFilters: {},
  setDashboardFilter: (key, value) =>
    set((s) => ({
      dashboardFilters: { ...s.dashboardFilters, [key]: value },
    })),
  clearDashboardFilters: () => set({ dashboardFilters: {} }),
});
