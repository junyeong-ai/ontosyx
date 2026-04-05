import { request } from "./client";

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

export function listChannels(): Promise<NotificationChannel[]> {
  return request("/notifications/channels");
}

export function createChannel(data: {
  name: string;
  channel_type: string;
  config: Record<string, unknown>;
  events: string[];
}): Promise<NotificationChannel> {
  return request("/notifications/channels", {
    method: "POST",
    body: JSON.stringify(data),
  });
}

export function updateChannel(
  id: string,
  data: Partial<{
    name: string;
    config: Record<string, unknown>;
    events: string[];
    enabled: boolean;
  }>,
): Promise<void> {
  return request(`/notifications/channels/${id}`, {
    method: "PATCH",
    body: JSON.stringify(data),
  });
}

export function deleteChannel(id: string): Promise<void> {
  return request(`/notifications/channels/${id}`, { method: "DELETE" });
}

export function testChannel(
  id: string,
): Promise<{ success: boolean; error?: string }> {
  return request(`/notifications/channels/${id}/test`, { method: "POST" });
}

export function listLogs(limit?: number): Promise<NotificationLog[]> {
  const params = limit ? `?limit=${limit}` : "";
  return request(`/notifications/log${params}`);
}
