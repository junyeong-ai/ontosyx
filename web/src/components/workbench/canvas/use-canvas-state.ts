import { useShallow } from "zustand/react/shallow";
import { useAppStore } from "@/lib/store";

/**
 * Consolidated canvas state selector.
 *
 * Replaces ~13 individual `useAppStore()` calls in `ontology-canvas.tsx`
 * with a single shallow-compared subscription, reducing unnecessary re-renders.
 */
export function useCanvasState() {
  return useAppStore(
    useShallow((s) => ({
      ontology: s.ontology,
      select: s.select,
      clearSelection: s.clearSelection,
      highlightedBindings: s.highlightedBindings,
      setHighlightedBindings: s.setHighlightedBindings,
      lastReconcileReport: s.lastReconcileReport,
      activeDiffOverlay: s.activeDiffOverlay,
      nodeGroups: s.nodeGroups,
      restoreNodeGroups: s.restoreNodeGroups,
      neighborhoodFocus: s.neighborhoodFocus,
      setNeighborhoodFocus: s.setNeighborhoodFocus,
      applyCommand: s.applyCommand,
      setActiveProject: s.setActiveProject,
      setOntology: s.setOntology,
    })),
  );
}
