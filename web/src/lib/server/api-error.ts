import { NextResponse } from "next/server";

type ApiErrorClass = "client_error" | "server_error";
type ApiErrorParams = Record<string, unknown>;

export function apiErrorResponse(
  status: number,
  code: string,
  errorClass: ApiErrorClass,
  params: ApiErrorParams = {},
): NextResponse {
  return NextResponse.json(
    {
      error: {
        code,
        class: errorClass,
        params,
      },
    },
    { status },
  );
}
