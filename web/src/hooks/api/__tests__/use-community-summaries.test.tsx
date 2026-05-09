import { act, renderHook, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import {
  deleteCommunitySummary,
  listCommunitySummaries,
  searchCommunitySummaries,
  upsertCommunitySummary,
} from "@/lib/api/community-summaries";

import {
  communitySummaryKeys,
  useCommunitySummaries,
  useDeleteCommunitySummary,
  useSearchCommunitySummaries,
  useUpsertCommunitySummary,
} from "../use-community-summaries";

vi.mock("@/lib/api/community-summaries", () => ({
  deleteCommunitySummary: vi.fn(),
  listCommunitySummaries: vi.fn(),
  searchCommunitySummaries: vi.fn(),
  upsertCommunitySummary: vi.fn(),
}));

function makeWrapper() {
  const qc = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={qc}>{children}</QueryClientProvider>
  );
  return { qc, wrapper };
}

const emptyList = { items: [] };
const savedSummary = {
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
};

describe("community summary hooks", () => {
  beforeEach(() => {
    vi.mocked(listCommunitySummaries).mockReset();
    vi.mocked(searchCommunitySummaries).mockReset();
    vi.mocked(upsertCommunitySummary).mockReset();
    vi.mocked(deleteCommunitySummary).mockReset();
  });

  it("lists canonical community summaries with a stable cache key", async () => {
    vi.mocked(listCommunitySummaries).mockResolvedValue(emptyList);

    const { wrapper } = makeWrapper();
    const { result } = renderHook(() => useCommunitySummaries(), { wrapper });

    await waitFor(() => expect(result.current.data).toEqual(emptyList));
    expect(listCommunitySummaries).toHaveBeenCalledTimes(1);
    expect(result.current.dataUpdatedAt).toBeGreaterThan(0);
  });

  it("does not search when the query is blank", () => {
    const { wrapper } = makeWrapper();

    renderHook(() => useSearchCommunitySummaries({ q: "   ", topK: 10 }), {
      wrapper,
    });

    expect(searchCommunitySummaries).not.toHaveBeenCalled();
  });

  it("searches with the provided query parameters", async () => {
    vi.mocked(searchCommunitySummaries).mockResolvedValue(emptyList);

    const { wrapper } = makeWrapper();
    const params = { q: "VIP customer", topK: 25 };
    const { result } = renderHook(() => useSearchCommunitySummaries(params), {
      wrapper,
    });

    await waitFor(() => expect(result.current.data).toEqual(emptyList));
    expect(searchCommunitySummaries).toHaveBeenCalledWith(params);
  });

  it("invalidates all community summary queries after upsert", async () => {
    vi.mocked(upsertCommunitySummary).mockResolvedValue(savedSummary);
    const { qc, wrapper } = makeWrapper();
    qc.setQueryData(communitySummaryKeys.list(), emptyList);

    const { result } = renderHook(() => useUpsertCommunitySummary(), {
      wrapper,
    });

    await act(async () => {
      await result.current.mutateAsync({
        community_id: "leiden:0:7",
        level: 0,
        member_entity_kinds: ["NodeType"],
        member_logical_ids: ["nt_customer"],
        title: "Premium customers",
        summary: "VIP customers with high-value orders.",
      });
    });

    expect(qc.getQueryState(communitySummaryKeys.list())?.isInvalidated).toBe(
      true,
    );
  });

  it("invalidates all community summary queries after delete", async () => {
    vi.mocked(deleteCommunitySummary).mockResolvedValue(undefined);
    const { qc, wrapper } = makeWrapper();
    qc.setQueryData(communitySummaryKeys.list(), emptyList);

    const { result } = renderHook(() => useDeleteCommunitySummary(), {
      wrapper,
    });

    await act(async () => {
      await result.current.mutateAsync("018f0000-0000-7000-8000-000000000001");
    });

    expect(deleteCommunitySummary).toHaveBeenCalledWith(
      "018f0000-0000-7000-8000-000000000001",
    );
    expect(qc.getQueryState(communitySummaryKeys.list())?.isInvalidated).toBe(
      true,
    );
  });
});
