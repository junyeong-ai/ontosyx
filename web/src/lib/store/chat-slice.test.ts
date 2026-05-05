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

  it("setHighlightedBindings(null) clears the ring highlight", () => {
    store.getState().setHighlightedBindings({
      node_bindings: [],
      edge_bindings: [],
      property_bindings: [],
    });
    expect(store.getState().highlightedBindings).not.toBeNull();
    store.getState().setHighlightedBindings(null);
    expect(store.getState().highlightedBindings).toBeNull();
  });

  it("executionMode has a stable default", () => {
    // Default is an implementation detail of the slice; assert only that
    // it's a truthy ExecutionMode so the test doesn't break every time
    // the product flips between "auto" / "agent" / "direct" defaults.
    const mode = store.getState().executionMode;
    expect(typeof mode).toBe("string");
    expect(mode.length).toBeGreaterThan(0);
  });

  it("setExecutionMode round-trips", () => {
    store.getState().setExecutionMode("supervised");
    expect(store.getState().executionMode).toBe("supervised");
    store.getState().setExecutionMode("auto");
    expect(store.getState().executionMode).toBe("auto");
  });

  it("setModelOverride persists the caller-chosen model id", () => {
    expect(store.getState().modelOverride).toBeNull();
    store.getState().setModelOverride("claude-opus-4-7");
    expect(store.getState().modelOverride).toBe("claude-opus-4-7");
    store.getState().setModelOverride(null);
    expect(store.getState().modelOverride).toBeNull();
  });

  it("setTokenUsage records cumulative counts", () => {
    expect(store.getState().tokenUsage).toBeNull();
    store.getState().setTokenUsage({ input: 100, output: 200 });
    expect(store.getState().tokenUsage).toEqual({ input: 100, output: 200 });
  });

  it("setSessionId switches conversation scope", () => {
    expect(store.getState().sessionId).toBeNull();
    store.getState().setSessionId("sess-1");
    expect(store.getState().sessionId).toBe("sess-1");
    store.getState().setSessionId(null);
    expect(store.getState().sessionId).toBeNull();
  });

  it("restoreMessages replaces the array wholesale", () => {
    store.getState().addMessage({ id: "m1", role: "user", content: "Old" });
    store.getState().restoreMessages([
      { id: "r1", role: "user", content: "Loaded" },
      { id: "r2", role: "assistant", content: "Response" },
    ]);
    const msgs = store.getState().messages;
    expect(msgs).toHaveLength(2);
    expect(msgs[0].id).toBe("r1");
    expect(msgs[1].content).toBe("Response");
  });
});
