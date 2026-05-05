import { describe, it, expect, beforeEach } from "vitest";
import { createStore } from "zustand";
import type { AppStore } from "./types";
import { createOntologySlice } from "./ontology-slice";
import { createChatSlice } from "./chat-slice";
import { createChromeSlice } from "./chrome-slice";
import { createSelectionSlice } from "./selection-slice";
import { createDashboardSlice } from "./dashboard-slice";
import { createOntologyDraftSlice } from "./ontology-draft-slice";
import { createVerificationSlice } from "./verification-slice";

function createTestStore() {
  return createStore<AppStore>()((...a) => ({
    ...createOntologySlice(...a),
    ...createChatSlice(...a),
    ...createOntologyDraftSlice(...a),
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

  // -------------------------------------------------------------------
  // Dashboard-scoped type-filter cross-widget coordination
  // -------------------------------------------------------------------

  it("dashboardTypeFilters defaults to empty object", () => {
    expect(store.getState().dashboardTypeFilters).toEqual({});
  });

  it("toggleDashboardType adds a type when absent", () => {
    store.getState().toggleDashboardType("d1", "Person");
    expect(store.getState().dashboardTypeFilters).toEqual({ d1: ["Person"] });
  });

  it("toggleDashboardType removes a type when present", () => {
    store.getState().toggleDashboardType("d1", "Person");
    store.getState().toggleDashboardType("d1", "Person");
    expect(store.getState().dashboardTypeFilters).toEqual({ d1: [] });
  });

  it("toggleDashboardType isolates different dashboards", () => {
    store.getState().toggleDashboardType("d1", "Person");
    store.getState().toggleDashboardType("d2", "Company");
    expect(store.getState().dashboardTypeFilters).toEqual({
      d1: ["Person"],
      d2: ["Company"],
    });
  });

  it("setDashboardTypeHidden is idempotent on matching state", () => {
    store.getState().setDashboardTypeHidden("d1", "Person", true);
    const snapshot = store.getState().dashboardTypeFilters;
    store.getState().setDashboardTypeHidden("d1", "Person", true);
    expect(store.getState().dashboardTypeFilters).toBe(snapshot);
  });

  it("setDashboardTypeHidden false removes a hidden type", () => {
    store.getState().setDashboardTypeHidden("d1", "Person", true);
    store.getState().setDashboardTypeHidden("d1", "Person", false);
    expect(store.getState().dashboardTypeFilters.d1).toEqual([]);
  });

  it("clearDashboardTypes drops the dashboard entry", () => {
    store.getState().toggleDashboardType("d1", "Person");
    store.getState().toggleDashboardType("d2", "Company");
    store.getState().clearDashboardTypes("d1");
    expect(store.getState().dashboardTypeFilters).toEqual({ d2: ["Company"] });
  });

  it("clearDashboardTypes is a no-op on absent key", () => {
    const snapshot = store.getState().dashboardTypeFilters;
    store.getState().clearDashboardTypes("never-existed");
    expect(store.getState().dashboardTypeFilters).toBe(snapshot);
  });
});
