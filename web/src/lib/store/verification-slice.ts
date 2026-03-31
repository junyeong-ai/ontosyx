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

  loadVerifications: async (ontologyId) => {
    set({ verificationsLoading: true });
    try {
      const data = await listVerifications(ontologyId);
      set({ verifications: data, verificationsLoading: false });
    } catch {
      set({ verificationsLoading: false });
    }
  },

  verifyElement: async (ontologyId, elementId, elementKind, notes) => {
    await verifyElement(ontologyId, {
      element_id: elementId,
      element_kind: elementKind,
      review_notes: notes,
    });
    // Refetch to get server-authoritative state (includes verified_by_name)
    const data = await listVerifications(ontologyId);
    set({ verifications: data });
  },

  revokeVerification: async (ontologyId, elementId) => {
    await revokeVerification(ontologyId, elementId);
    const data = await listVerifications(ontologyId);
    set({ verifications: data });
  },

  clearVerifications: () => set({ verifications: [], verificationsLoading: false }),
});
