import { describe, expect, it } from "vitest";
import { ApiError, isApiError } from "../client";

describe("ApiError.kind", () => {
  it("maps 0 to network", () => {
    expect(new ApiError({ status: 0 }).kind()).toBe("network");
  });

  it("maps 401 to unauthorized", () => {
    expect(new ApiError({ status: 401 }).kind()).toBe("unauthorized");
  });

  it("maps 403 to forbidden", () => {
    expect(new ApiError({ status: 403 }).kind()).toBe("forbidden");
  });

  it("maps 404 to notFound", () => {
    expect(new ApiError({ status: 404 }).kind()).toBe("notFound");
  });

  it("maps 409 / 410 to conflict", () => {
    expect(new ApiError({ status: 409 }).kind()).toBe("conflict");
    expect(new ApiError({ status: 410 }).kind()).toBe("conflict");
  });

  it("maps 429 to rateLimited", () => {
    expect(new ApiError({ status: 429 }).kind()).toBe("rateLimited");
  });

  it("maps any 5xx to serverError", () => {
    expect(new ApiError({ status: 500 }).kind()).toBe("serverError");
    expect(new ApiError({ status: 503 }).kind()).toBe("serverError");
    expect(new ApiError({ status: 599 }).kind()).toBe("serverError");
  });

  it("maps unmapped 4xx (400, 422) to clientError", () => {
    expect(new ApiError({ status: 400 }).kind()).toBe("clientError");
    expect(new ApiError({ status: 422 }).kind()).toBe("clientError");
    expect(new ApiError({ status: 415 }).kind()).toBe("clientError");
  });

  it("maps unknown statuses to unknown", () => {
    expect(new ApiError({ status: 600 }).kind()).toBe("unknown");
  });
});

describe("ApiError.isClientError", () => {
  it("returns true for 4xx", () => {
    expect(new ApiError({ status: 400 }).isClientError()).toBe(true);
    expect(new ApiError({ status: 499 }).isClientError()).toBe(true);
  });
  it("returns false for non-4xx", () => {
    expect(new ApiError({ status: 500 }).isClientError()).toBe(false);
    expect(new ApiError({ status: 0 }).isClientError()).toBe(false);
  });
});

describe("ApiError.localize", () => {
  it("looks up errors.<code> with params", () => {
    const err = new ApiError({
      status: 404,
      code: "not_found",
      params: { entity: "Project" },
    });
    const captured: Array<[string, unknown]> = [];
    const t = (key: string, values?: Record<string, unknown>) => {
      captured.push([key, values]);
      return `${key}:${JSON.stringify(values)}`;
    };
    expect(err.localize(t)).toBe("not_found:{\"entity\":\"Project\"}");
    expect(captured[0]?.[0]).toBe("not_found");
  });

  it("falls back to errors.unknown if the catalog throws", () => {
    const err = new ApiError({
      status: 422,
      code: "code_not_in_catalog",
    });
    const t = (key: string) => {
      if (key === "code_not_in_catalog") {
        throw new Error("missing translation");
      }
      return "fallback";
    };
    expect(err.localize(t)).toBe("fallback");
  });

  it("falls back to dev message when no code present", () => {
    const err = new ApiError({ status: 500, devMessage: "boom" });
    expect(err.localize(() => "x")).toBe("boom");
  });
});

describe("isApiError", () => {
  it("recognises ApiError instances", () => {
    expect(isApiError(new ApiError({}))).toBe(true);
  });
  it("rejects everything else", () => {
    expect(isApiError(new Error("x"))).toBe(false);
    expect(isApiError("x")).toBe(false);
    expect(isApiError(null)).toBe(false);
    expect(isApiError({})).toBe(false);
  });
});
