import { describe, it, expect, beforeEach } from "vitest";
import { createStore } from "zustand";
import type { AppStore } from "./types";
import { createOntologySlice } from "./ontology-slice";
import { createChatSlice } from "./chat-slice";
import { createChromeSlice } from "./chrome-slice";
import { createSelectionSlice } from "./selection-slice";
import { createDashboardSlice } from "./dashboard-slice";
import { createProjectSlice } from "./project-slice";
import { createVerificationSlice } from "./verification-slice";

function createTestStore() {
  return createStore<AppStore>()((...a) => ({
    ...createOntologySlice(...a),
    ...createChatSlice(...a),
    ...createProjectSlice(...a),
    ...createChromeSlice(...a),
    ...createSelectionSlice(...a),
    ...createDashboardSlice(...a),
    ...createVerificationSlice(...a),
  }));
}

describe("DashboardSlice", () => {
  let store: ReturnType<typeof createTestStore>;

  beforeEach(() => {
    store = createTestStore();
  });

  it("activeDashboardId defaults to null", () => {
    expect(store.getState().activeDashboardId).toBeNull();
  });

  it("setActiveDashboardId stores id", () => {
    store.getState().setActiveDashboardId("d1");
    expect(store.getState().activeDashboardId).toBe("d1");
  });

  it("setActiveDashboardId clears with null", () => {
    store.getState().setActiveDashboardId("d1");
    store.getState().setActiveDashboardId(null);
    expect(store.getState().activeDashboardId).toBeNull();
  });

  it("dashboardFilters defaults to empty", () => {
    expect(store.getState().dashboardFilters).toEqual({});
  });

  it("setDashboardFilter adds filter", () => {
    store.getState().setDashboardFilter("category", "Electronics");
    expect(store.getState().dashboardFilters).toEqual({ category: "Electronics" });
  });

  it("setDashboardFilter accumulates filters", () => {
    store.getState().setDashboardFilter("category", "Electronics");
    store.getState().setDashboardFilter("region", "Asia");
    expect(store.getState().dashboardFilters).toEqual({
      category: "Electronics",
      region: "Asia",
    });
  });

  it("clearDashboardFilters resets all", () => {
    store.getState().setDashboardFilter("category", "Electronics");
    store.getState().setDashboardFilter("region", "Asia");
    store.getState().clearDashboardFilters();
    expect(store.getState().dashboardFilters).toEqual({});
  });

  it("dashboardWidgetCount defaults to 0", () => {
    expect(store.getState().dashboardWidgetCount).toBe(0);
  });

  it("setDashboardWidgetCount updates count", () => {
    store.getState().setDashboardWidgetCount(5);
    expect(store.getState().dashboardWidgetCount).toBe(5);
  });
});
