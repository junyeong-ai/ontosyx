// Barrel re-export — preserves `import { ... } from "@/lib/api"` paths.

export { ApiError, PROXY_BASE, DESIGN_TIMEOUT, DEFAULT_TIMEOUT, fetchWithTimeout } from "./client";
export type { FetchOptions, RetryOptions } from "./client";

export { consumeSSEStream } from "./sse";
export { isPendingReconcile, normalizeQueryResult } from "./normalization";

export * from "./chat";
export * from "./queries";
export * from "./projects";
export * from "./dashboards";
export * from "./ontology";
export * from "./admin";
export * from "./perspectives";
export * from "./workspaces";
export * from "./models";

// Type re-exports from @/types/api (for backward compat with old import paths)
export type { HealthResponse, InsightSuggestion, SessionMessage } from "@/types/api";
