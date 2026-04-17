"use client";

import { QueryClientProvider } from "@tanstack/react-query";
import dynamic from "next/dynamic";
import { getQueryClient } from "@/lib/query/client";

// Why: Devtools are dev-only. Dynamic-importing them keeps the production
// bundle tree-shaken clean without a manual `process.env.NODE_ENV` guard
// inside the render path.
const ReactQueryDevtools =
  process.env.NODE_ENV === "production"
    ? () => null
    : dynamic(
        () =>
          import("@tanstack/react-query-devtools").then(
            (m) => m.ReactQueryDevtools,
          ),
        { ssr: false },
      );

export function QueryProvider({ children }: { children: React.ReactNode }) {
  // Why: call getQueryClient() inside the component so that on the client we
  // read the browser singleton (preserved across remounts), and on the server
  // we get a throwaway per-request instance.
  const client = getQueryClient();

  return (
    <QueryClientProvider client={client}>
      {children}
      <ReactQueryDevtools initialIsOpen={false} />
    </QueryClientProvider>
  );
}
