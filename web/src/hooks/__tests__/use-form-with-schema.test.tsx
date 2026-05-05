import { describe, expect, it, vi } from "vitest";
import { act, renderHook } from "@testing-library/react";
import { z } from "zod";

import { useFormWithSchema } from "../use-form-with-schema";

const SCHEMA = z.object({
  name: z.string().min(1, "Name is required"),
  email: z.string().email("Invalid email"),
  age: z.number().int().min(18, "Must be 18+"),
});

describe("useFormWithSchema", () => {
  it("calls onValid with the parsed value when input passes the schema", async () => {
    const onValid = vi.fn();
    const { result } = renderHook(() =>
      useFormWithSchema({ schema: SCHEMA, onValid }),
    );
    let ok = false;
    await act(async () => {
      ok = (await result.current.submit({
        name: "Hyejin",
        email: "h@example.com",
        age: 30,
      })) as boolean;
    });
    expect(ok).toBe(true);
    expect(onValid).toHaveBeenCalledWith({
      name: "Hyejin",
      email: "h@example.com",
      age: 30,
    });
    expect(result.current.errors).toEqual({});
  });

  it("collects per-field errors and skips onValid when input fails", async () => {
    const onValid = vi.fn();
    const { result } = renderHook(() =>
      useFormWithSchema({ schema: SCHEMA, onValid }),
    );
    let ok: boolean | undefined;
    await act(async () => {
      ok = (await result.current.submit({
        name: "",
        email: "not-an-email",
        age: 12,
      })) as boolean;
    });
    expect(ok).toBe(false);
    expect(onValid).not.toHaveBeenCalled();
    expect(result.current.errors.name).toBe("Name is required");
    expect(result.current.errors.email).toBe("Invalid email");
    expect(result.current.errors.age).toBe("Must be 18+");
  });

  it("clearErrors(path) drops only the named field error", async () => {
    const { result } = renderHook(() =>
      useFormWithSchema({ schema: SCHEMA, onValid: () => {} }),
    );
    await act(async () => {
      await result.current.submit({ name: "", email: "x", age: 5 });
    });
    expect(Object.keys(result.current.errors).sort()).toEqual([
      "age",
      "email",
      "name",
    ]);
    act(() => {
      result.current.clearErrors("name");
    });
    expect(result.current.errors.name).toBeUndefined();
    expect(result.current.errors.email).toBe("Invalid email");
  });

  it("clearErrors() with no arg wipes every error", async () => {
    const { result } = renderHook(() =>
      useFormWithSchema({ schema: SCHEMA, onValid: () => {} }),
    );
    await act(async () => {
      await result.current.submit({ name: "", email: "x", age: 5 });
    });
    expect(Object.keys(result.current.errors).length).toBeGreaterThan(0);
    act(() => {
      result.current.clearErrors();
    });
    expect(result.current.errors).toEqual({});
  });

  it("flips `pending` true while an async onValid resolves", async () => {
    let resolveOnValid: () => void;
    const slow = new Promise<void>((resolve) => {
      resolveOnValid = resolve;
    });
    const { result } = renderHook(() =>
      useFormWithSchema({
        schema: SCHEMA,
        onValid: () => slow,
      }),
    );
    let submitPromise: Promise<boolean> | undefined;
    act(() => {
      submitPromise = Promise.resolve(
        result.current.submit({
          name: "x",
          email: "h@example.com",
          age: 30,
        }),
      ).then((v) => Boolean(v));
    });
    // Validation has already passed; pending is true while we wait.
    await act(async () => {
      await Promise.resolve();
    });
    expect(result.current.pending).toBe(true);
    await act(async () => {
      resolveOnValid!();
      await submitPromise;
    });
    expect(result.current.pending).toBe(false);
  });

  it("encodes top-level (path-empty) errors under `_form`", async () => {
    const refineSchema = SCHEMA.refine(
      (v) => v.name !== v.email,
      { message: "Name and email must differ" },
    );
    const { result } = renderHook(() =>
      useFormWithSchema({ schema: refineSchema, onValid: () => {} }),
    );
    await act(async () => {
      await result.current.submit({
        name: "h@example.com",
        email: "h@example.com",
        age: 30,
      });
    });
    expect(result.current.errors._form).toBe("Name and email must differ");
  });
});
