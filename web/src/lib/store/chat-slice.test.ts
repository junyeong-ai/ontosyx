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

describe("ChatSlice", () => {
  let store: ReturnType<typeof createTestStore>;

  beforeEach(() => {
    store = createTestStore();
  });

  it("starts with empty messages", () => {
    expect(store.getState().messages).toEqual([]);
    expect(store.getState().isLoading).toBe(false);
  });

  it("addMessage appends to messages array", () => {
    store.getState().addMessage({
      id: "m1",
      role: "user",
      content: "Hello",
    });

    expect(store.getState().messages).toHaveLength(1);
    expect(store.getState().messages[0].content).toBe("Hello");
  });

  it("updateMessage patches existing message", () => {
    store.getState().addMessage({
      id: "m1",
      role: "assistant",
      content: "",
      isStreaming: true,
    });

    store.getState().updateMessage("m1", {
      content: "Response text",
      isStreaming: false,
    });

    const msg = store.getState().messages[0];
    expect(msg.content).toBe("Response text");
    expect(msg.isStreaming).toBe(false);
    expect(msg.role).toBe("assistant"); // unchanged fields preserved
  });

  it("updateMessage ignores unknown id", () => {
    store.getState().addMessage({ id: "m1", role: "user", content: "Hi" });
    store.getState().updateMessage("nonexistent", { content: "X" });

    expect(store.getState().messages).toHaveLength(1);
    expect(store.getState().messages[0].content).toBe("Hi");
  });

  it("clearMessages resets array", () => {
    store.getState().addMessage({ id: "m1", role: "user", content: "Hi" });
    store.getState().addMessage({ id: "m2", role: "assistant", content: "Hello" });
    store.getState().clearMessages();

    expect(store.getState().messages).toEqual([]);
  });

  it("setIsLoading toggles loading state", () => {
    store.getState().setIsLoading(true);
    expect(store.getState().isLoading).toBe(true);

    store.getState().setIsLoading(false);
    expect(store.getState().isLoading).toBe(false);
  });

  it("setCommandBarInput sets pending input", () => {
    store.getState().setCommandBarInput("Fix this gap");

    expect(store.getState().pendingCommandBarInput).toBe("Fix this gap");
  });

  it("takeCommandBarInput returns and clears pending", () => {
    store.getState().setCommandBarInput("Fix this gap");

    const input = store.getState().takeCommandBarInput();
    expect(input).toBe("Fix this gap");
    expect(store.getState().pendingCommandBarInput).toBeNull();
  });

  it("takeCommandBarInput returns null when no pending", () => {
    const input = store.getState().takeCommandBarInput();
    expect(input).toBeNull();
  });

  it("highlightedBindings defaults to null", () => {
    expect(store.getState().highlightedBindings).toBeNull();
  });

  it("setHighlightedBindings stores bindings", () => {
    const bindings = {
      node_bindings: [],
      edge_bindings: [],
      property_bindings: [],
    };
    store.getState().setHighlightedBindings(bindings);
    expect(store.getState().highlightedBindings).toEqual(bindings);
  });
});
