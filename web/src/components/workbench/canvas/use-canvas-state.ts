import { useShallow } from "zustand/react/shallow";
import { useAppStore } from "@/lib/store";

/**
 * Consolidated canvas state selector.
 *
 * Returns a shallow-compared snapshot of the store slices the main
 * canvas component still needs directly (ontology + selection + highlight +
 * neighborhood setter). Other slices (viewport, selection effects, commands,
 * keyboard, context menu) consume the store themselves via dedicated hooks.
 */
export function useCanvasState() {
  return useAppStore(
    useShallow((s) => ({
      ontology: s.ontology,
      selectOne: s.selectOne,
      toggleSelection: s.toggleSelection,
      selectMany: s.selectMany,
      clearSelection: s.clearSelection,
      setHighlightedBindings: s.setHighlightedBindings,
      setNeighborhoodFocus: s.setNeighborhoodFocus,
    })),
  );
}
