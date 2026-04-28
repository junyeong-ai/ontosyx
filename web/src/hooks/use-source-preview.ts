import { useQuery } from "@tanstack/react-query";

import { request } from "@/lib/api/client";
import type {
  ProjectSource,
  PreviewSourceResponse,
} from "@/types/projects";

const PREVIEW_PATH = "/projects/source-preview";

/**
 * Fetch the cheap table listing for an arbitrary `ProjectSource`.
 *
 * Calls `POST /api/projects/source-preview` (designer-role).
 * Returns `null` data while `source` is `null` (lets callers gate
 * the request on user input). Cache key includes the full source
 * payload so two sources that differ only in `schema` produce
 * separate caches.
 */
export function useSourcePreview(source: ProjectSource | null) {
  return useQuery<PreviewSourceResponse | null>({
    queryKey: ["projects", "source-preview", source],
    queryFn: async () => {
      if (!source) return null;
      // The wire shape is `#[serde(flatten)] source: ProjectSource`,
      // so the source's discriminator + fields sit at the top level
      // rather than nested under a `source` key.
      const res = await request<{ data: PreviewSourceResponse }>(
        PREVIEW_PATH,
        {
          method: "POST",
          body: JSON.stringify(source),
        },
      );
      return res.data;
    },
    enabled: source !== null,
    // Preview results reflect the source's catalog at fetch time —
    // safe to cache for a few minutes during a selection session.
    staleTime: 60_000,
  });
}
