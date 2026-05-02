// ---------------------------------------------------------------------------
// Dashboard types
// ---------------------------------------------------------------------------

export interface Dashboard {
  id: string;
  workspace_id: string;
  user_id: string;
  name: string;
  description: string | null;
  layout: DashboardWidgetPosition[];
  is_public: boolean;
  share_token: string | null;
  shared_at: string | null;
  share_expires_at: string | null;
  created_at: string;
  updated_at: string;
}

export interface DashboardWidgetPosition {
  widget_id: string;
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface DashboardCreateRequest {
  name: string;
  description?: string;
}

export interface DashboardUpdateRequest {
  name?: string;
  description?: string;
  layout?: unknown[];
  is_public?: boolean;
}

// --- Dashboard Widgets ---

export interface DashboardWidget {
  id: string;
  dashboard_id: string;
  title: string;
  widget_type: string;
  query: string | null;
  widget_spec: Record<string, unknown>;
  position: { x: number; y: number; w: number; h: number };
  refresh_interval_secs: number | null;
  thresholds?: {
    warning?: number;
    critical?: number;
    direction?: "above" | "below";
  };
  last_result: Record<string, unknown> | null;
  last_refreshed: string | null;
  created_at: string;
}

export interface WidgetCreateRequest {
  title: string;
  widget_type: string;
  query?: string;
  widget_spec?: Record<string, unknown>;
  position?: { x: number; y: number; w: number; h: number };
  refresh_interval_secs?: number;
  thresholds?: {
    warning?: number;
    critical?: number;
    direction?: "above" | "below";
  };
}

/**
 * PATCH-style widget update — every field is optional. Mirror of
 * `WidgetCreateRequest` plus the omitted layout fields the inspector
 * doesn't expose.
 */
export interface WidgetUpdateRequest {
  title?: string;
  widget_type?: string;
  query?: string;
  refresh_interval_secs?: number;
  thresholds?: {
    warning?: number;
    critical?: number;
    direction?: "above" | "below";
  };
}
