import type { StateCreator } from "zustand";
import type { AppStore, DashboardSlice } from "./types";

export const createDashboardSlice: StateCreator<AppStore, [], [], DashboardSlice> = (set) => ({
  activeDashboardId: null,
  setActiveDashboardId: (id) => set({ activeDashboardId: id }),
  dashboardFilters: {},
  setDashboardFilter: (key, value) =>
    set((s) => ({
      dashboardFilters: { ...s.dashboardFilters, [key]: value },
    })),
  clearDashboardFilters: () => set({ dashboardFilters: {} }),

  dashboardTypeFilters: {},
  toggleDashboardType: (dashboardId, type) =>
    set((s) => {
      const current = s.dashboardTypeFilters[dashboardId] ?? [];
      const next = current.includes(type)
        ? current.filter((t) => t !== type)
        : [...current, type];
      return {
        dashboardTypeFilters: {
          ...s.dashboardTypeFilters,
          [dashboardId]: next,
        },
      };
    }),
  setDashboardTypeHidden: (dashboardId, type, hidden) =>
    set((s) => {
      const current = s.dashboardTypeFilters[dashboardId] ?? [];
      const already = current.includes(type);
      // Fast-path: the requested state already matches — don't
      // churn the store (every re-render on unrelated widgets
      // subscribing to this slice would repaint otherwise).
      if (hidden === already) return s;
      const next = hidden
        ? [...current, type]
        : current.filter((t) => t !== type);
      return {
        dashboardTypeFilters: {
          ...s.dashboardTypeFilters,
          [dashboardId]: next,
        },
      };
    }),
  clearDashboardTypes: (dashboardId) =>
    set((s) => {
      if (!(dashboardId in s.dashboardTypeFilters)) return s;
      // Drop the key entirely rather than set it to `[]` — keeps the
      // "absent = no filter" invariant the hook relies on, and lets
      // the persist layer shed state for dashboards that end up
      // filter-free across sessions.
      const next = { ...s.dashboardTypeFilters };
      delete next[dashboardId];
      return { dashboardTypeFilters: next };
    }),
});
