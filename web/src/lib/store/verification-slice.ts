import type { StateCreator } from "zustand";
import type { AppStore, VerificationSlice } from "./types";
import { listVerifications, verifyElement, revokeVerification } from "@/lib/api/ontology";

export const createVerificationSlice: StateCreator<
  AppStore,
  [],
  [],
  VerificationSlice
> = (set) => ({
  verifications: [],
  verificationsLoading: false,

  loadVerifications: async () => {
    set({ verificationsLoading: true });
    try {
      const data = await listVerifications();
      set({ verifications: data, verificationsLoading: false });
    } catch {
      set({ verificationsLoading: false });
    }
  },

  verifyElement: async (elementId, elementKind, notes) => {
    await verifyElement({
      element_id: elementId,
      element_kind: elementKind,
      review_notes: notes,
    });
    // Refetch to get server-authoritative state (includes verified_by_name)
    const data = await listVerifications();
    set({ verifications: data });
  },

  revokeVerification: async (elementId) => {
    await revokeVerification(elementId);
    const data = await listVerifications();
    set({ verifications: data });
  },

  clearVerifications: () => set({ verifications: [], verificationsLoading: false }),
});
