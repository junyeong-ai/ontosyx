import type { components } from "@/types/api.generated";
import { request } from "./client";

// `NotificationChannel` / `NotificationLog` flow through `ox_store` and
// the OpenAPI surface keeps them opaque (`body = Object`); this module
// owns the FE-side typed contract for both shapes.

export interface NotificationChannel {
  id: string;
  workspace_id: string;
  name: string;
  channel_type: string;
  config: Record<string, unknown>;
  events: string[];
  enabled: boolean;
  created_at: string;
  updated_at: string;
}

export interface NotificationLog {
  id: string;
  workspace_id: string;
  channel_id: string;
  event_type: string;
  subject: string;
  body: string;
  status: string;
  error: string | null;
  created_at: string;
}

export type CreateChannelRequest =
  components["schemas"]["CreateChannelRequest"];
export type UpdateChannelRequest =
  components["schemas"]["UpdateChannelRequest"];
export type TestChannelResponse =
  components["schemas"]["TestChannelResponse"];

export function listChannels(): Promise<NotificationChannel[]> {
  return request("/notifications/channels");
}

export function createChannel(
  data: CreateChannelRequest,
): Promise<NotificationChannel> {
  return request("/notifications/channels", {
    method: "POST",
    body: JSON.stringify(data),
  });
}

export function updateChannel(
  id: string,
  data: UpdateChannelRequest,
): Promise<void> {
  return request(`/notifications/channels/${id}`, {
    method: "PATCH",
    body: JSON.stringify(data),
  });
}

export function deleteChannel(id: string): Promise<void> {
  return request(`/notifications/channels/${id}`, { method: "DELETE" });
}

export function testChannel(id: string): Promise<TestChannelResponse> {
  return request(`/notifications/channels/${id}/test`, { method: "POST" });
}

export function listLogs(limit?: number): Promise<NotificationLog[]> {
  const params = limit ? `?limit=${limit}` : "";
  return request(`/notifications/log${params}`);
}
