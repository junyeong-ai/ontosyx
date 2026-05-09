import type { components } from "./api.generated";
import type { ClientPage } from "./pagination";

export type Dashboard = components["schemas"]["Dashboard"];
export type DashboardPage = ClientPage<components["schemas"]["DashboardPage"]>;
export type DashboardLayoutItem = components["schemas"]["DashboardLayoutItem"];
export type DashboardWidget = components["schemas"]["DashboardWidget"];
export type DashboardWidgetPosition = components["schemas"]["DashboardWidgetPosition"];
export type DashboardWidgetThresholds = components["schemas"]["DashboardWidgetThresholds"];
export type DashboardCreateRequest = components["schemas"]["CreateDashboardRequest"];
export type DashboardUpdateRequest = components["schemas"]["UpdateDashboardRequest"];
export type WidgetCreateRequest = components["schemas"]["CreateWidgetRequest"];
export type WidgetUpdateRequest = components["schemas"]["WidgetUpdateRequest"];
