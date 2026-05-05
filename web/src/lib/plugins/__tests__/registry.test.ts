import { describe, expect, it, vi } from "vitest";

import { PluginRegistry } from "../registry";

interface Item {
  id: string;
  weight?: number;
  payload: string;
}

describe("PluginRegistry.register", () => {
  it("appends a fresh id and returns an unregister thunk", () => {
    const reg = new PluginRegistry<Item>();
    const off = reg.register({ id: "a", payload: "1" });
    expect(reg.list()).toHaveLength(1);
    off();
    expect(reg.list()).toHaveLength(0);
  });

  it("replaces an existing id without churning order", () => {
    const reg = new PluginRegistry<Item>();
    reg.register({ id: "a", payload: "1" });
    reg.register({ id: "b", payload: "2" });
    reg.register({ id: "a", payload: "1-updated" });
    expect(reg.list().map((i) => i.id)).toEqual(["a", "b"]);
    expect(reg.get("a")?.payload).toBe("1-updated");
  });

  it("returns a stable snapshot until a mutation invalidates it", () => {
    const reg = new PluginRegistry<Item>();
    reg.register({ id: "a", payload: "1" });
    const snap1 = reg.list();
    const snap2 = reg.list();
    expect(snap1).toBe(snap2);
    reg.register({ id: "b", payload: "2" });
    const snap3 = reg.list();
    expect(snap3).not.toBe(snap1);
  });

  it("unregister thunk only removes the active payload", () => {
    const reg = new PluginRegistry<Item>();
    const off1 = reg.register({ id: "a", payload: "1" });
    reg.register({ id: "a", payload: "1-updated" });
    off1();
    // Replacing then calling the OLD off1 must NOT remove the new entry.
    expect(reg.get("a")?.payload).toBe("1-updated");
  });
});

describe("PluginRegistry.subscribe", () => {
  it("notifies on register / unregister", () => {
    const reg = new PluginRegistry<Item>();
    const listener = vi.fn();
    const off = reg.subscribe(listener);
    reg.register({ id: "a", payload: "1" });
    expect(listener).toHaveBeenCalledTimes(1);
    reg.unregister("a");
    expect(listener).toHaveBeenCalledTimes(2);
    off();
    reg.register({ id: "b", payload: "2" });
    expect(listener).toHaveBeenCalledTimes(2);
  });
});

describe("PluginRegistry positional insertion", () => {
  it("inserts before a named neighbour", () => {
    const reg = new PluginRegistry<Item>();
    reg.register({ id: "a", payload: "1" });
    reg.register({ id: "c", payload: "3" });
    reg.register({ id: "b", payload: "2" }, { before: "c" });
    expect(reg.list().map((i) => i.id)).toEqual(["a", "b", "c"]);
  });

  it("inserts after a named neighbour", () => {
    const reg = new PluginRegistry<Item>();
    reg.register({ id: "a", payload: "1" });
    reg.register({ id: "c", payload: "3" });
    reg.register({ id: "b", payload: "2" }, { after: "a" });
    expect(reg.list().map((i) => i.id)).toEqual(["a", "b", "c"]);
  });

  it("falls back to append when the neighbour id is unregistered", () => {
    const reg = new PluginRegistry<Item>();
    reg.register({ id: "a", payload: "1" });
    reg.register(
      { id: "b", payload: "2" },
      { before: "ghost" },
    );
    expect(reg.list().map((i) => i.id)).toEqual(["a", "b"]);
  });

  it("preserves position when re-registering the same id", () => {
    const reg = new PluginRegistry<Item>();
    reg.register({ id: "a", payload: "1" });
    reg.register({ id: "b", payload: "2" });
    reg.register({ id: "c", payload: "3" });
    // Re-register `a` with `{ before: "c" }` — position must NOT
    // change; the option is for first registration only.
    reg.register({ id: "a", payload: "1-updated" }, { before: "c" });
    expect(reg.list().map((i) => i.id)).toEqual(["a", "b", "c"]);
  });
});

describe("PluginRegistry compare", () => {
  it("sorts the snapshot when a comparator is supplied", () => {
    const reg = new PluginRegistry<Item>({
      compare: (a, b) => (a.weight ?? 0) - (b.weight ?? 0),
    });
    reg.register({ id: "c", payload: "third", weight: 30 });
    reg.register({ id: "a", payload: "first", weight: 10 });
    reg.register({ id: "b", payload: "second", weight: 20 });
    expect(reg.list().map((i) => i.id)).toEqual(["a", "b", "c"]);
  });

  it("preserves insertion order without a comparator", () => {
    const reg = new PluginRegistry<Item>();
    reg.register({ id: "c", payload: "1" });
    reg.register({ id: "a", payload: "2" });
    reg.register({ id: "b", payload: "3" });
    expect(reg.list().map((i) => i.id)).toEqual(["c", "a", "b"]);
  });
});
