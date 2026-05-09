import type { components } from "@/types/api.generated";
import { request } from "./client";

export type NotificationChannel =
  components["schemas"]["NotificationChannel"];
export type NotificationChannelType =
  components["schemas"]["NotificationChannelType"];
export type NotificationLog = components["schemas"]["NotificationLog"];
export type WebhookNotificationConfig =
  components["schemas"]["WebhookNotificationConfig"];
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
