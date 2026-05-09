"use client";

import {
  useMutation,
  useQuery,
  useQueryClient,
  type UseMutationOptions,
  type UseQueryOptions,
} from "@tanstack/react-query";

import {
  deleteCommunitySummary,
  listCommunitySummaries,
  searchCommunitySummaries,
  upsertCommunitySummary,
  type CommunitySummaryResponse,
  type ListCommunitySummariesResponse,
  type SearchCommunitySummariesParams,
  type UpsertCommunitySummaryRequest,
} from "@/lib/api/community-summaries";

export const communitySummaryKeys = {
  all: ["community-summaries"] as const,
  lists: () => [...communitySummaryKeys.all, "list"] as const,
  list: () => [...communitySummaryKeys.lists(), "canonical"] as const,
  searches: () => [...communitySummaryKeys.all, "search"] as const,
  search: (params: SearchCommunitySummariesParams) =>
    [...communitySummaryKeys.searches(), params] as const,
};

export function useCommunitySummaries(
  options?: Omit<
    UseQueryOptions<ListCommunitySummariesResponse>,
    "queryKey" | "queryFn"
  >,
) {
  return useQuery({
    queryKey: communitySummaryKeys.list(),
    queryFn: () => listCommunitySummaries(),
    ...options,
  });
}

export function useSearchCommunitySummaries(
  params: SearchCommunitySummariesParams,
  options?: Omit<
    UseQueryOptions<ListCommunitySummariesResponse>,
    "queryKey" | "queryFn"
  >,
) {
  const enabled =
    (options?.enabled ?? true) && params.q.trim().length > 0;
  return useQuery({
    queryKey: communitySummaryKeys.search(params),
    queryFn: () => searchCommunitySummaries(params),
    ...options,
    enabled,
  });
}

export function useUpsertCommunitySummary(
  options?: UseMutationOptions<
    CommunitySummaryResponse,
    Error,
    UpsertCommunitySummaryRequest
  >,
) {
  const queryClient = useQueryClient();
  const { onSuccess, ...rest } = options ?? {};
  return useMutation<
    CommunitySummaryResponse,
    Error,
    UpsertCommunitySummaryRequest
  >({
    ...rest,
    mutationFn: (body) => upsertCommunitySummary(body),
    onSuccess: (...args) => {
      queryClient.invalidateQueries({ queryKey: communitySummaryKeys.all });
      onSuccess?.(...args);
    },
  });
}

export function useDeleteCommunitySummary(
  options?: UseMutationOptions<void, Error, string>,
) {
  const queryClient = useQueryClient();
  const { onSuccess, ...rest } = options ?? {};
  return useMutation<void, Error, string>({
    ...rest,
    mutationFn: (id) => deleteCommunitySummary(id),
    onSuccess: (...args) => {
      queryClient.invalidateQueries({ queryKey: communitySummaryKeys.all });
      onSuccess?.(...args);
    },
  });
}
