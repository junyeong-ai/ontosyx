"use client";

/**
 * Hidden SVG defs that register UML-style markers (filled diamond,
 * hollow diamond) for the canvas.
 *
 * React Flow lets each edge reference an arbitrary `markerStart` /
 * `markerEnd` URL, but only if a `<marker>` of that id exists
 * somewhere in the same document. Mounting this component once at
 * the canvas root attaches the registry without forcing every edge
 * type to inline its own defs.
 *
 * UML convention: in `Whole ◆── Part`, the diamond sits at the
 * **whole** end (the source of our directed edge), so we expose the
 * markers via `markerStart`. The plain Association edge keeps the
 * existing arrowhead `markerEnd` so semantic differences read
 * unambiguously: an arrow points at the dependant, a diamond points
 * at the owner.
 */
export const EDGE_MARKER_COMPOSITION = "edge-marker-composition";
export const EDGE_MARKER_AGGREGATION = "edge-marker-aggregation";

export function EdgeKindMarkers() {
  return (
    <svg
      aria-hidden="true"
      className="pointer-events-none absolute h-0 w-0"
      style={{ position: "absolute" }}
    >
      <defs>
        {/* Filled diamond — Composition (strong ownership). */}
        <marker
          id={EDGE_MARKER_COMPOSITION}
          viewBox="0 0 12 12"
          refX="11"
          refY="6"
          markerWidth="12"
          markerHeight="12"
          orient="auto-start-reverse"
        >
          <path d="M 0 6 L 6 0 L 12 6 L 6 12 Z" fill="#475569" />
        </marker>

        {/* Hollow diamond — Aggregation (loose containment). */}
        <marker
          id={EDGE_MARKER_AGGREGATION}
          viewBox="0 0 12 12"
          refX="11"
          refY="6"
          markerWidth="12"
          markerHeight="12"
          orient="auto-start-reverse"
        >
          <path
            d="M 0 6 L 6 0 L 12 6 L 6 12 Z"
            fill="#ffffff"
            stroke="#475569"
            strokeWidth="1.5"
          />
        </marker>
      </defs>
    </svg>
  );
}
