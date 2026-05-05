import { request } from "./client";
import type {
  CreateInsightRequest,
  InsightDef,
  InsightListPage,
  UpdateInsightRequest,
} from "@/types/api";

const BASE = "/insights";

export async function createInsight(req: CreateInsightRequest): Promise<InsightDef> {
  const res = await request<{ insight: InsightDef }>(BASE, {
    method: "POST",
    body: JSON.stringify(req),
  });
  return res.insight;
}

export async function updateInsight(
  id: string,
  req: UpdateInsightRequest,
): Promise<InsightDef> {
  const res = await request<{ insight: InsightDef }>(
    `${BASE}/${encodeURIComponent(id)}`,
    { method: "PUT", body: JSON.stringify(req) },
  );
  return res.insight;
}

export async function getInsight(id: string): Promise<InsightDef> {
  const res = await request<{ insight: InsightDef }>(
    `${BASE}/${encodeURIComponent(id)}`,
  );
  return res.insight;
}

export interface ListInsightsParams {
  me?: boolean;
  /** Restrict to insights that carry at least one of these
   *  `GlossaryTermId` anchors (server array-overlap). Empty / unset
   *  means "any". */
  conceptAnchors?: string[];
  /** Same multi-value semantics as `conceptAnchors`, on freeform
   *  tag strings. */
  tags?: string[];
  cursor?: string;
  limit?: number;
}

export async function listInsights(
  params: ListInsightsParams = {},
): Promise<InsightListPage> {
  const qs = new URLSearchParams();
  if (params.me === false) qs.set("me", "false");
  for (const anchor of params.conceptAnchors ?? []) {
    qs.append("concept_anchor", anchor);
  }
  for (const tag of params.tags ?? []) {
    qs.append("tag", tag);
  }
  if (params.cursor) qs.set("cursor", params.cursor);
  if (params.limit) qs.set("limit", String(params.limit));
  const suffix = qs.toString() ? `?${qs}` : "";
  return request<InsightListPage>(`${BASE}${suffix}`);
}

export async function deleteInsight(id: string): Promise<void> {
  await request(`${BASE}/${encodeURIComponent(id)}`, { method: "DELETE" });
}
