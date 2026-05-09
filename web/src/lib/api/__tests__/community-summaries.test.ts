import { beforeEach, describe, expect, it, vi } from "vitest";

import {
  deleteCommunitySummary,
  listCommunitySummaries,
  searchCommunitySummaries,
  upsertCommunitySummary,
} from "../community-summaries";

function jsonResponse(data: unknown, init?: ResponseInit): Response {
  return new Response(JSON.stringify({ data }), {
    status: 200,
    headers: { "Content-Type": "application/json" },
    ...init,
  });
}

describe("community summary API", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    window.localStorage.clear();
    window.sessionStorage.setItem("ontosyx.auth_enabled", "true");
    window.localStorage.setItem("ontosyx.workspace_id", "ws-test");
  });

  it("lists community summaries from the canonical ontology endpoint", async () => {
    const fetchSpy = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValue(jsonResponse({ items: [] }));

    await expect(listCommunitySummaries()).resolves.toEqual({ items: [] });
    expect(fetchSpy.mock.calls[0]?.[0]).toBe(
      "/api/proxy/ontology/communities",
    );
    expect((fetchSpy.mock.calls[0]?.[1] as RequestInit).method).toBeUndefined();
  });

  it("searches with encoded query and top_k", async () => {
    const fetchSpy = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValue(jsonResponse({ items: [] }));

    await searchCommunitySummaries({ q: "VIP customer", topK: 25 });

    const url = String(fetchSpy.mock.calls[0]?.[0]);
    expect(url).toBe(
      "/api/proxy/ontology/communities/search?q=VIP+customer&top_k=25",
    );
  });

  it("upserts community summaries with the typed request body", async () => {
    const fetchSpy = vi.spyOn(globalThis, "fetch").mockResolvedValue(
      jsonResponse({
        summary: {
          id: "018f0000-0000-7000-8000-000000000001",
          workspace_id: "018f0000-0000-7000-8000-000000000002",
          ontology_version_id: "018f0000-0000-7000-8000-000000000003",
          community_id: "leiden:0:7",
          level: 0,
          member_entity_kinds: ["NodeType"],
          member_logical_ids: ["nt_customer"],
          title: "Premium customers",
          summary: "VIP customers with high-value orders.",
          generated_at: "2026-05-07T00:00:00Z",
        },
      }),
    );

    await upsertCommunitySummary({
      community_id: "leiden:0:7",
      level: 0,
      member_entity_kinds: ["NodeType"],
      member_logical_ids: ["nt_customer"],
      title: "Premium customers",
      summary: "VIP customers with high-value orders.",
    });

    const init = fetchSpy.mock.calls[0]?.[1] as RequestInit;
    expect(fetchSpy.mock.calls[0]?.[0]).toBe(
      "/api/proxy/ontology/communities",
    );
    expect(init.method).toBe("POST");
    expect(JSON.parse(String(init.body))).toMatchObject({
      community_id: "leiden:0:7",
      member_logical_ids: ["nt_customer"],
    });
  });

  it("deletes by encoded id and accepts 204", async () => {
    const fetchSpy = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValue(new Response(null, { status: 204 }));

    await expect(deleteCommunitySummary("id/with/slash")).resolves.toBeUndefined();

    expect(fetchSpy.mock.calls[0]?.[0]).toBe(
      "/api/proxy/ontology/communities/id%2Fwith%2Fslash",
    );
    expect((fetchSpy.mock.calls[0]?.[1] as RequestInit).method).toBe("DELETE");
  });
});
