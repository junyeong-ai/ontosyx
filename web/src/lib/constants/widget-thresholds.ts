/**
 * Auto-detection thresholds for widget type selection.
 *
 * Used by widget-renderer.tsx to choose the best visualization
 * for a given data shape. Extracted here for maintainability —
 * tune these values to adjust detection sensitivity.
 */

/** Minimum rows required for scatter plot (need enough points for pattern) */
export const MIN_SCATTER_ROWS = 5;

/** Minimum rows for histogram (need enough data for binning) */
export const MIN_HISTOGRAM_ROWS = 5;

/** Minimum columns for combo chart (label + 2+ metrics) */
export const MIN_COMBO_COLUMNS = 3;

/** Maximum rows for funnel (beyond this, funnel loses meaning) */
export const MAX_FUNNEL_ROWS = 20;

/** Maximum histogram bins */
export const MAX_HISTOGRAM_BINS = 20;

/** Bubble size range for scatter with z-axis [min, max] in pixels */
export const SCATTER_BUBBLE_RANGE: [number, number] = [20, 400];
