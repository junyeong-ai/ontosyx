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

/** Typed query parameter for dashboard widgets */
export interface WidgetParameter {
  name: string;
  label?: string;
  type: "string" | "number" | "boolean";
  default_value?: string | number | boolean;
}

export interface DashboardWidget {
  id: string;
  dashboard_id: string;
  title: string;
  widget_type: string;
  query: string | null;
  widget_spec: Record<string, unknown>;
  position: { x: number; y: number; w: number; h: number };
  refresh_interval_secs: number | null;
  parameters?: WidgetParameter[];
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
