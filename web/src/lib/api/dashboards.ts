import type {
  WidgetCreateRequest,
  DashboardCreateRequest,
  CursorPage,
  Dashboard,
  DashboardWidget,
  DashboardUpdateRequest,
} from "@/types/api";
import { request } from "./client";

// ---------------------------------------------------------------------------
// Dashboard CRUD
// ---------------------------------------------------------------------------

export async function createDashboard(
  req: DashboardCreateRequest,
): Promise<Dashboard> {
  return request("/dashboards", {
    method: "POST",
    body: JSON.stringify(req),
  });
}

export async function listDashboards(params?: {
  cursor?: string;
  limit?: number;
}): Promise<CursorPage<Dashboard>> {
  const qs = new URLSearchParams();
  if (params?.cursor) qs.set("cursor", params.cursor);
  if (params?.limit) qs.set("limit", String(params.limit));
  const query = qs.toString();
  return request(`/dashboards${query ? `?${query}` : ""}`);
}

export async function getDashboard(id: string): Promise<Dashboard> {
  return request(`/dashboards/${encodeURIComponent(id)}`);
}

export async function updateDashboard(
  id: string,
  req: DashboardUpdateRequest,
): Promise<void> {
  await request(`/dashboards/${encodeURIComponent(id)}`, {
    method: "PATCH",
    body: JSON.stringify(req),
  });
}

export async function deleteDashboard(id: string): Promise<void> {
  await request(`/dashboards/${encodeURIComponent(id)}`, {
    method: "DELETE",
  });
}

// ---------------------------------------------------------------------------
// Dashboard Widgets
// ---------------------------------------------------------------------------

export async function addWidget(
  dashboardId: string,
  req: WidgetCreateRequest,
): Promise<DashboardWidget> {
  return request(`/dashboards/${encodeURIComponent(dashboardId)}/widgets`, {
    method: "POST",
    body: JSON.stringify(req),
  });
}

export async function listWidgets(
  dashboardId: string,
): Promise<DashboardWidget[]> {
  return request(`/dashboards/${encodeURIComponent(dashboardId)}/widgets`);
}

export async function deleteWidget(
  dashboardId: string,
  widgetId: string,
): Promise<void> {
  await request(
    `/dashboards/${encodeURIComponent(dashboardId)}/widgets/${encodeURIComponent(widgetId)}`,
    { method: "DELETE" },
  );
}

export async function updateWidget(
  dashboardId: string,
  widgetId: string,
  req: {
    title?: string;
    widget_type?: string;
    query?: string;
    refresh_interval_secs?: number;
    thresholds?: { warning?: number; critical?: number; direction?: "above" | "below" };
  },
): Promise<void> {
  await request(
    `/dashboards/${encodeURIComponent(dashboardId)}/widgets/${encodeURIComponent(widgetId)}`,
    { method: "PATCH", body: JSON.stringify(req) },
  );
}

// ---------------------------------------------------------------------------
// Dashboard Sharing
// ---------------------------------------------------------------------------

export async function shareDashboard(
  id: string,
): Promise<{ share_token: string }> {
  return request(`/dashboards/${encodeURIComponent(id)}/share`, {
    method: "POST",
  });
}

export async function unshareDashboard(id: string): Promise<void> {
  await request(`/dashboards/${encodeURIComponent(id)}/share`, {
    method: "DELETE",
  });
}
