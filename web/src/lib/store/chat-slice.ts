import type { StateCreator } from "zustand";
import type { AppStore, ChatSlice } from "./types";

export const createChatSlice: StateCreator<AppStore, [], [], ChatSlice> = (
  set,
  get,
) => ({
  messages: [],
  isLoading: false,
  sessionId: null,
  setSessionId: (id) => set({ sessionId: id }),
  addMessage: (msg) =>
    set((s) => ({ messages: [...s.messages, msg] })),
  updateMessage: (id, patch) =>
    set((s) => ({
      messages: s.messages.map((m) =>
        m.id === id ? { ...m, ...patch } : m,
      ),
    })),
  restoreMessages: (messages) => set({ messages }),
  clearMessages: () => set({ messages: [], sessionId: null, tokenUsage: null }),
  setIsLoading: (isLoading) => set({ isLoading }),

  tokenUsage: null,
  setTokenUsage: (usage) => set({ tokenUsage: usage }),

  highlightedBindings: null,
  setHighlightedBindings: (bindings) => set({ highlightedBindings: bindings }),

  pendingCommandBarInput: null,
  setCommandBarInput: (input) => {
    set({ pendingCommandBarInput: input });
  },
  takeCommandBarInput: () => {
    const input = get().pendingCommandBarInput;
    if (input) set({ pendingCommandBarInput: null });
    return input;
  },

  executionMode: "auto",
  setExecutionMode: (mode) => set({ executionMode: mode }),

  modelOverride: null,
  setModelOverride: (model) => set({ modelOverride: model }),
});
