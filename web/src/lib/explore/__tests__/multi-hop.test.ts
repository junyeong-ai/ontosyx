import { describe, it, expect, vi, beforeEach } from "vitest";
import type { ExpandNeighbor, ExpandResult } from "@/lib/api/queries";

// Mock `expandNode` at the module boundary — the BFS helper reads
// from it, so stubbing here drives the test graph deterministically.
const expandNodeMock = vi.fn<(id: string) => Promise<ExpandResult>>();
vi.mock("@/lib/api/queries", async (orig) => {
  const actual = await orig<typeof import("@/lib/api/queries")>();
  return {
    ...actual,
    expandNode: (id: string) => expandNodeMock(id),
  };
});

// Import AFTER the mock so the module-level binding picks up the stub.
const { expandMultiHop } = await import("@/lib/explore/multi-hop");

function neighbor(id: string, kind = "REL"): ExpandNeighbor {
  return {
    element_id: id,
    labels: ["Node"],
    props: {},
    relationship_type: kind,
    direction: "outgoing",
  };
}

beforeEach(() => {
  expandNodeMock.mockReset();
});

describe("expandMultiHop", () => {
  it("depth 1 returns the direct expansion result", async () => {
    expandNodeMock.mockResolvedValueOnce({
      source_id: "root",
      neighbors: [neighbor("a"), neighbor("b")],
    });
    const result = await expandMultiHop("root", { depth: 1 });
    expect(result.map((n) => n.element_id)).toEqual(["a", "b"]);
    expect(expandNodeMock).toHaveBeenCalledTimes(1);
  });

  it("depth 2 BFS expands every hop-1 neighbour", async () => {
    expandNodeMock
      .mockResolvedValueOnce({
        source_id: "root",
        neighbors: [neighbor("a"), neighbor("b")],
      })
      .mockResolvedValueOnce({
        source_id: "a",
        neighbors: [neighbor("c")],
      })
      .mockResolvedValueOnce({
        source_id: "b",
        neighbors: [neighbor("d")],
      });

    const result = await expandMultiHop("root", { depth: 2 });
    // Hop-1 first (a, b), hop-2 next (c, d).
    expect(result.map((n) => n.element_id)).toEqual(["a", "b", "c", "d"]);
    expect(expandNodeMock).toHaveBeenCalledTimes(3);
  });

  it("dedups already-seen nodes across hops (cycle-safe)", async () => {
    // root → a; a → root, b; b → a
    expandNodeMock.mockImplementation((id: string) => {
      if (id === "root")
        return Promise.resolve({ source_id: id, neighbors: [neighbor("a")] });
      if (id === "a")
        return Promise.resolve({
          source_id: id,
          neighbors: [neighbor("root"), neighbor("b")],
        });
      if (id === "b")
        return Promise.resolve({
          source_id: id,
          neighbors: [neighbor("a")],
        });
      return Promise.resolve({ source_id: id, neighbors: [] });
    });

    const result = await expandMultiHop("root", { depth: 3 });
    expect(result.map((n) => n.element_id)).toEqual(["a", "b"]);
  });

  it("respects maxNodes cap", async () => {
    expandNodeMock.mockResolvedValueOnce({
      source_id: "root",
      neighbors: [
        neighbor("a"),
        neighbor("b"),
        neighbor("c"),
        neighbor("d"),
      ],
    });
    const result = await expandMultiHop("root", { depth: 1, maxNodes: 2 });
    expect(result.map((n) => n.element_id)).toEqual(["a", "b"]);
  });

  it("tolerates rejected expansions at a hop without aborting the whole walk", async () => {
    expandNodeMock
      .mockResolvedValueOnce({
        source_id: "root",
        neighbors: [neighbor("a"), neighbor("b")],
      })
      .mockRejectedValueOnce(new Error("flaky"))
      .mockResolvedValueOnce({
        source_id: "b",
        neighbors: [neighbor("c")],
      });
    const result = await expandMultiHop("root", { depth: 2 });
    expect(result.map((n) => n.element_id)).toEqual(["a", "b", "c"]);
  });
});
