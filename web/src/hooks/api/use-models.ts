"use client";

import {
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";

import {
  createModelConfig,
  createRoutingRule,
  deleteModelConfig,
  deleteRoutingRule,
  listModelOperations,
  listModelConfigs,
  listRoutingRules,
  testModelConfig,
  updateModelConfig,
  updateRoutingRule,
  type ModelConfig,
  type ModelRoutingRule,
  type TestModelRequest,
  type TestModelResponse,
} from "@/lib/api/models";

export const modelsKeys = {
  all: ["models"] as const,
  operations: () => [...modelsKeys.all, "operations"] as const,
  configs: () => [...modelsKeys.all, "configs"] as const,
  rules: () => [...modelsKeys.all, "rules"] as const,
};

export function useModelOperations() {
  return useQuery({
    queryKey: modelsKeys.operations(),
    queryFn: () => listModelOperations(),
  });
}

export function useModelConfigs() {
  return useQuery({
    queryKey: modelsKeys.configs(),
    queryFn: () => listModelConfigs(),
  });
}

export function useRoutingRules() {
  return useQuery({
    queryKey: modelsKeys.rules(),
    queryFn: () => listRoutingRules(),
  });
}

type ConfigInput = Parameters<typeof createModelConfig>[0];
type RuleInput = Parameters<typeof createRoutingRule>[0];

export function useCreateModelConfig() {
  const qc = useQueryClient();
  return useMutation<ModelConfig, Error, ConfigInput>({
    mutationFn: (req) => createModelConfig(req),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: modelsKeys.configs() });
    },
  });
}

export function useUpdateModelConfig() {
  const qc = useQueryClient();
  return useMutation<
    ModelConfig,
    Error,
    { id: string; patch: Parameters<typeof updateModelConfig>[1] }
  >({
    mutationFn: ({ id, patch }) => updateModelConfig(id, patch),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: modelsKeys.configs() });
    },
  });
}

export function useDeleteModelConfig() {
  const qc = useQueryClient();
  return useMutation<void, Error, string>({
    mutationFn: (id) => deleteModelConfig(id),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: modelsKeys.configs() });
      qc.invalidateQueries({ queryKey: modelsKeys.rules() });
    },
  });
}

export function useTestModelConfig() {
  return useMutation<TestModelResponse, Error, TestModelRequest>({
    mutationFn: (req) => testModelConfig(req),
  });
}

export function useCreateRoutingRule() {
  const qc = useQueryClient();
  return useMutation<ModelRoutingRule, Error, RuleInput>({
    mutationFn: (req) => createRoutingRule(req),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: modelsKeys.rules() });
    },
  });
}

export function useUpdateRoutingRule() {
  const qc = useQueryClient();
  return useMutation<
    ModelRoutingRule,
    Error,
    { id: string; patch: Parameters<typeof updateRoutingRule>[1] }
  >({
    mutationFn: ({ id, patch }) => updateRoutingRule(id, patch),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: modelsKeys.rules() });
    },
  });
}

export function useDeleteRoutingRule() {
  const qc = useQueryClient();
  return useMutation<void, Error, string>({
    mutationFn: (id) => deleteRoutingRule(id),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: modelsKeys.rules() });
    },
  });
}
