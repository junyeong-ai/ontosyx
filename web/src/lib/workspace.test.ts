import { beforeEach, afterEach, describe, it, expect, vi } from "vitest";

import { getWorkspaceId, setWorkspaceId } from "./workspace";

const STORAGE_KEY = "ontosyx.workspace_id";

const DEV_WS = "dev-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const STALE_WS = "stale-bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const USER_WS = "user-cccccccccccccccccccccccccccccccc";

beforeEach(() => {
  window.localStorage.clear();
});

afterEach(() => {
  vi.unstubAllEnvs();
});

describe("getWorkspaceId — dev mode (NEXT_PUBLIC_OX_DEV_WORKSPACE_ID set)", () => {
  beforeEach(() => {
    vi.stubEnv("NODE_ENV", "development");
    vi.stubEnv("NEXT_PUBLIC_OX_DEV_WORKSPACE_ID", DEV_WS);
  });

  it("returns the dev env when localStorage is empty", () => {
    expect(getWorkspaceId()).toBe(DEV_WS);
  });

  it("mirrors the dev env into localStorage on first read", () => {
    getWorkspaceId();
    expect(window.localStorage.getItem(STORAGE_KEY)).toBe(DEV_WS);
  });

  it("returns the dev env when localStorage already matches (no rewrite)", () => {
    window.localStorage.setItem(STORAGE_KEY, DEV_WS);
    const setSpy = vi.spyOn(Storage.prototype, "setItem");
    expect(getWorkspaceId()).toBe(DEV_WS);
    expect(setSpy).not.toHaveBeenCalled();
  });

  // Bug A regression: `dev.sh seed` regenerates the workspace UUID on
  // every boot. A stale cache from a previous seed would otherwise
  // shadow the fresh value and produce silent 404s on every
  // workspace-scoped endpoint.
  it("overwrites stale localStorage when the dev env has changed", () => {
    window.localStorage.setItem(STORAGE_KEY, STALE_WS);
    expect(getWorkspaceId()).toBe(DEV_WS);
    expect(window.localStorage.getItem(STORAGE_KEY)).toBe(DEV_WS);
  });
});

describe("getWorkspaceId — dev mode, no dev env", () => {
  beforeEach(() => {
    vi.stubEnv("NODE_ENV", "development");
    vi.stubEnv("NEXT_PUBLIC_OX_DEV_WORKSPACE_ID", "");
  });

  it("falls back to localStorage when the dev env is unset", () => {
    window.localStorage.setItem(STORAGE_KEY, USER_WS);
    expect(getWorkspaceId()).toBe(USER_WS);
  });

  it("returns undefined when neither env nor cache is populated", () => {
    expect(getWorkspaceId()).toBeUndefined();
  });
});

describe("getWorkspaceId — production mode", () => {
  beforeEach(() => {
    vi.stubEnv("NODE_ENV", "production");
    // Even if the dev env happens to be set, prod must ignore it —
    // the login flow owns `localStorage.ontosyx.workspace_id`.
    vi.stubEnv("NEXT_PUBLIC_OX_DEV_WORKSPACE_ID", DEV_WS);
  });

  it("ignores the dev env and reads localStorage", () => {
    window.localStorage.setItem(STORAGE_KEY, USER_WS);
    expect(getWorkspaceId()).toBe(USER_WS);
  });

  it("returns undefined when localStorage is empty", () => {
    expect(getWorkspaceId()).toBeUndefined();
  });
});

describe("setWorkspaceId", () => {
  it("writes the id to localStorage", () => {
    setWorkspaceId(USER_WS);
    expect(window.localStorage.getItem(STORAGE_KEY)).toBe(USER_WS);
  });

  it("clears the id and related keys when passed undefined", () => {
    window.localStorage.setItem(STORAGE_KEY, USER_WS);
    window.localStorage.setItem("ontosyx.workspace_name", "My Workspace");
    window.localStorage.setItem("ontosyx.workspace_role", "admin");
    setWorkspaceId(undefined);
    expect(window.localStorage.getItem(STORAGE_KEY)).toBeNull();
    expect(window.localStorage.getItem("ontosyx.workspace_name")).toBeNull();
    expect(window.localStorage.getItem("ontosyx.workspace_role")).toBeNull();
  });
});
