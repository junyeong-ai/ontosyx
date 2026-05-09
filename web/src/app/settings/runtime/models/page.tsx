"use client";

import { useState } from "react";
import { toast } from "@/components/ui/toast";
import { Heading } from "@/components/ui/heading";
import { useTranslations } from "next-intl";

import { Button } from "@/components/ui/button";
import { ErrorState } from "@/components/ui/error-state";
import { KpiCard } from "@/components/ui/kpi-card";
import { FormInput, SettingsSelect, SettingsSwitch } from "@/components/ui/form-input";
import { Skeleton, SkeletonTable } from "@/components/ui/skeleton";
import { SettingsPageShell } from "@/components/layout/settings-page-shell";
import { useConfirm } from "@/components/providers/confirm-provider";
import {
  useCreateModelConfig,
  useCreateRoutingRule,
  useDeleteModelConfig,
  useDeleteRoutingRule,
  useModelOperations,
  useModelConfigs,
  useRoutingRules,
  useTestModelConfig,
  useUpdateModelConfig,
  useUpdateRoutingRule,
} from "@/hooks/api/use-models";
import type { ModelConfig, ModelRoutingRule } from "@/lib/api/models";

// ---------------------------------------------------------------------------
// Form types
// ---------------------------------------------------------------------------

interface ConfigFormValues {
  name: string;
  provider: string;
  model_id: string;
  max_tokens: number;
  temperature: string;
  timeout_secs: number;
  cost_per_1m_input: string;
  cost_per_1m_output: string;
  daily_budget_usd: string;
  priority: number;
  enabled: boolean;
  api_key_env: string;
  region: string;
  base_url: string;
}

const EMPTY_CONFIG_FORM: ConfigFormValues = {
  name: "",
  provider: "anthropic",
  model_id: "",
  max_tokens: 4096,
  temperature: "",
  timeout_secs: 120,
  cost_per_1m_input: "",
  cost_per_1m_output: "",
  daily_budget_usd: "",
  priority: 0,
  enabled: true,
  api_key_env: "",
  region: "",
  base_url: "",
};

const PROVIDERS = [
  "anthropic",
  "openai",
  "bedrock",
  "vertex",
  "ollama",
  "custom",
] as const;

const OPERATIONS = [
  "*",
  "chat",
  "design_ontology",
  "refine_ontology",
  "resolve_cross_edges",
  "edit_ontology",
  "translate_query",
  "plan_load",
  "select_widget",
  "explain",
  "suggest_insights",
  "repo_navigate",
  "repo_analyze",
  "evaluation_judge",
  "evaluation_safety_judge",
] as const;

interface RuleFormValues {
  operation: string;
  model_config_id: string;
  priority: number;
  enabled: boolean;
}

const EMPTY_RULE_FORM: RuleFormValues = {
  operation: "*",
  model_config_id: "",
  priority: 0,
  enabled: true,
};

// ---------------------------------------------------------------------------
// Page
// ---------------------------------------------------------------------------

export default function ModelsSettingsPage() {
  const t = useTranslations("settings.runtime.models");
  const tCommon = useTranslations("common");
  const confirm = useConfirm();

  const operationsQuery = useModelOperations();
  const configsQuery = useModelConfigs();
  const rulesQuery = useRoutingRules();
  const createConfig = useCreateModelConfig();
  const updateConfig = useUpdateModelConfig();
  const deleteConfig = useDeleteModelConfig();
  const testConfig = useTestModelConfig();
  const createRule = useCreateRoutingRule();
  const updateRule = useUpdateRoutingRule();
  const deleteRule = useDeleteRoutingRule();

  const configs = configsQuery.data ?? [];
  const rules = rulesQuery.data ?? [];
  const operations = operationsQuery.data?.map((op) => op.key) ?? [...OPERATIONS];
  const loading = configsQuery.isLoading || rulesQuery.isLoading;
  const failed = configsQuery.isError || rulesQuery.isError;

  // Config form state
  const [configFormOpen, setConfigFormOpen] = useState(false);
  const [editingConfigId, setEditingConfigId] = useState<string | null>(null);
  const [configForm, setConfigForm] = useState<ConfigFormValues>(EMPTY_CONFIG_FORM);
  const [configErrors, setConfigErrors] = useState<Record<string, string>>({});
  const [testingId, setTestingId] = useState<string | null>(null);

  // Rule form state
  const [ruleFormOpen, setRuleFormOpen] = useState(false);
  const [editingRuleId, setEditingRuleId] = useState<string | null>(null);
  const [ruleForm, setRuleForm] = useState<RuleFormValues>(EMPTY_RULE_FORM);

  // ---- Config CRUD ----

  const openCreateConfig = () => {
    setEditingConfigId(null);
    setConfigForm(EMPTY_CONFIG_FORM);
    setConfigErrors({});
    setConfigFormOpen(true);
  };

  const openEditConfig = (c: ModelConfig) => {
    setEditingConfigId(c.id);
    setConfigForm({
      name: c.name,
      provider: c.provider,
      model_id: c.model_id,
      max_tokens: c.max_tokens,
      temperature: c.temperature !== null ? String(c.temperature) : "",
      timeout_secs: c.timeout_secs,
      cost_per_1m_input:
        c.cost_per_1m_input !== null ? String(c.cost_per_1m_input) : "",
      cost_per_1m_output:
        c.cost_per_1m_output !== null ? String(c.cost_per_1m_output) : "",
      daily_budget_usd:
        c.daily_budget_usd !== null ? String(c.daily_budget_usd) : "",
      priority: c.priority,
      enabled: c.enabled,
      api_key_env: c.api_key_env ?? "",
      region: c.region ?? "",
      base_url: c.base_url ?? "",
    });
    setConfigErrors({});
    setConfigFormOpen(true);
  };

  const cancelConfigForm = () => {
    setConfigFormOpen(false);
    setEditingConfigId(null);
    setConfigForm(EMPTY_CONFIG_FORM);
    setConfigErrors({});
  };

  const clearConfigError = (field: string) => {
    if (!configErrors[field]) return;
    setConfigErrors((prev) => {
      const next = { ...prev };
      delete next[field];
      return next;
    });
  };

  const validateConfig = (): boolean => {
    const e: Record<string, string> = {};
    if (!configForm.name.trim()) e.name = t("validation.required");
    if (!configForm.provider.trim()) e.provider = t("validation.required");
    if (!configForm.model_id.trim()) e.model_id = t("validation.required");
    if (configForm.max_tokens < 1) e.max_tokens = t("validation.minOne");
    if (configForm.temperature) {
      const v = Number(configForm.temperature);
      if (v < 0 || v > 2) e.temperature = t("validation.temperatureRange");
    }
    setConfigErrors(e);
    return Object.keys(e).length === 0;
  };

  const handleSubmitConfig = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!validateConfig()) return;

    const body = {
      name: configForm.name.trim(),
      provider: configForm.provider,
      model_id: configForm.model_id.trim(),
      max_tokens: configForm.max_tokens,
      temperature: configForm.temperature ? Number(configForm.temperature) : null,
      timeout_secs: configForm.timeout_secs,
      cost_per_1m_input: configForm.cost_per_1m_input
        ? Number(configForm.cost_per_1m_input)
        : null,
      cost_per_1m_output: configForm.cost_per_1m_output
        ? Number(configForm.cost_per_1m_output)
        : null,
      daily_budget_usd: configForm.daily_budget_usd
        ? Number(configForm.daily_budget_usd)
        : null,
      priority: configForm.priority,
      enabled: configForm.enabled,
      api_key_env: configForm.api_key_env.trim() || null,
      region: configForm.region.trim() || null,
      base_url: configForm.base_url.trim() || null,
    };

    try {
      if (editingConfigId) {
        await updateConfig.mutateAsync({ id: editingConfigId, patch: body });
        toast.success(t("toast.configUpdated"));
      } else {
        await createConfig.mutateAsync(body);
        toast.success(t("toast.configCreated"));
      }
      cancelConfigForm();
    } catch {
      toast.error(
        editingConfigId
          ? t("toast.configUpdateFailed")
          : t("toast.configCreateFailed"),
      );
    }
  };

  const handleDeleteConfig = async (id: string) => {
    const config = configs.find((c) => c.id === id);
    const ok = await confirm({
      title: t("confirm.deleteConfigTitle", { name: config?.name ?? id }),
      description: t("confirm.deleteConfigDescription"),
      variant: "danger",
    });
    if (!ok) return;
    try {
      await deleteConfig.mutateAsync(id);
      toast.success(t("toast.configDeleted"));
    } catch {
      toast.error(t("toast.configDeleteFailed"));
    }
  };

  const handleToggleEnabled = async (c: ModelConfig) => {
    try {
      await updateConfig.mutateAsync({
        id: c.id,
        patch: { enabled: !c.enabled },
      });
    } catch {
      toast.error(t("toast.toggleFailed"));
    }
  };

  const handleTestConfig = async (config: ModelConfig) => {
    setTestingId(config.id);
    try {
      const result = await testConfig.mutateAsync({
        provider: config.provider,
        model_id: config.model_id,
        api_key_env: config.api_key_env,
        region: config.region,
        base_url: config.base_url,
      });
      if (result.ok) {
        toast.success(t("toast.testSuccess", { message: result.message }));
      } else {
        toast.error(t("toast.testFailed", { error: result.message }));
      }
    } catch {
      toast.error(t("toast.testError"));
    } finally {
      setTestingId(null);
    }
  };

  // ---- Rule CRUD ----

  const openCreateRule = () => {
    setEditingRuleId(null);
    setRuleForm({ ...EMPTY_RULE_FORM, model_config_id: configs[0]?.id ?? "" });
    setRuleFormOpen(true);
  };

  const openEditRule = (r: ModelRoutingRule) => {
    setEditingRuleId(r.id);
    setRuleForm({
      operation: r.operation,
      model_config_id: r.model_config_id,
      priority: r.priority,
      enabled: r.enabled,
    });
    setRuleFormOpen(true);
  };

  const cancelRuleForm = () => {
    setRuleFormOpen(false);
    setEditingRuleId(null);
    setRuleForm(EMPTY_RULE_FORM);
  };

  const handleSubmitRule = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!ruleForm.model_config_id) return;
    try {
      if (editingRuleId) {
        await updateRule.mutateAsync({ id: editingRuleId, patch: ruleForm });
        toast.success(t("toast.ruleUpdated"));
      } else {
        await createRule.mutateAsync(ruleForm);
        toast.success(t("toast.ruleCreated"));
      }
      cancelRuleForm();
    } catch {
      toast.error(
        editingRuleId
          ? t("toast.ruleUpdateFailed")
          : t("toast.ruleCreateFailed"),
      );
    }
  };

  const handleDeleteRule = async (id: string) => {
    const rule = rules.find((r) => r.id === id);
    const ok = await confirm({
      title: t("confirm.deleteRuleTitle", { operation: rule?.operation ?? id }),
      description: t("confirm.deleteRuleDescription"),
      variant: "danger",
    });
    if (!ok) return;
    try {
      await deleteRule.mutateAsync(id);
      toast.success(t("toast.ruleDeleted"));
    } catch {
      toast.error(t("toast.ruleDeleteFailed"));
    }
  };

  // ---- Render ----

  const configName = (id: string) =>
    configs.find((c) => c.id === id)?.name ?? id.slice(0, 8);

  if (loading) {
    return (
      <SettingsPageShell title={t("title")} subtitle={t("description")}>
        <div className="grid grid-cols-3 gap-4">
          {Array.from({ length: 3 }, (_, i) => (
            <div
              key={i}
              className="rounded-lg border border-divider bg-surface-base p-4"
            >
              <Skeleton className="mb-2 h-3 w-1/2" />
              <Skeleton className="h-7 w-12" />
            </div>
          ))}
        </div>
        <div className="mt-6">
          <SkeletonTable rows={4} cols={6} />
        </div>
      </SettingsPageShell>
    );
  }

  if (failed) {
    return (
      <SettingsPageShell title={t("title")} subtitle={t("description")}>
        <div className="py-12">
          <ErrorState
            title={tCommon("loadError.title")}
            description={tCommon("loadError.description")}
            onRetry={() => {
              configsQuery.refetch();
              rulesQuery.refetch();
            }}
            retryLabel={tCommon("retry")}
          />
        </div>
      </SettingsPageShell>
    );
  }

  const savingConfig = createConfig.isPending || updateConfig.isPending;
  const savingRule = createRule.isPending || updateRule.isPending;

  return (
    <SettingsPageShell
      title={t("title")}
      subtitle={t("description")}
      actions={
        !configFormOpen && (
          <Button variant="primary" size="sm" onClick={openCreateConfig}>
            {t("addAction")}
          </Button>
        )
      }
    >
      <div className="grid grid-cols-3 gap-4">
        <KpiCard
          tone="success"
          label={t("kpis.enabled")}
          value={configs.filter((c) => c.enabled).length}
        />
        <KpiCard
          tone="neutral"
          label={t("kpis.disabled")}
          value={configs.filter((c) => !c.enabled).length}
        />
        <KpiCard tone="info" label={t("kpis.rules")} value={rules.length} />
      </div>

      {configFormOpen && (
        <ConfigForm
          form={configForm}
          setForm={setConfigForm}
          errors={configErrors}
          clearError={clearConfigError}
          isEditing={!!editingConfigId}
          saving={savingConfig}
          onSubmit={handleSubmitConfig}
          onCancel={cancelConfigForm}
        />
      )}

      <div
        className="-mx-6 mt-6 overflow-x-auto px-6"
        tabIndex={0}
        role="region"
        aria-label={t("table.regionLabel")}
      >
        <table className="w-full min-w-[640px] text-sm">
          <thead>
            <tr className="border-b border-divider text-start text-xs font-medium uppercase text-foreground-muted">
              <th className="py-3 pe-6">{t("table.name")}</th>
              <th className="py-3 pe-6">{t("table.provider")}</th>
              <th className="py-3 pe-6">{t("table.model")}</th>
              <th className="py-3 pe-6">{t("table.priority")}</th>
              <th className="py-3 pe-6">{t("table.enabled")}</th>
              <th className="py-3 pe-6 text-end">{t("table.actions")}</th>
            </tr>
          </thead>
          <tbody>
            {configs.map((c) => (
              <tr key={c.id} className="border-b border-divider-soft">
                <td className="py-3 pe-6 font-medium text-foreground-strong">
                  {c.name}
                </td>
                <td className="py-3 pe-6 text-foreground-muted">
                  <ProviderBadge provider={c.provider} />
                </td>
                <td className="py-3 pe-6 font-mono text-xs text-foreground-muted">
                  {c.model_id}
                </td>
                <td className="py-3 pe-6 text-foreground-muted">{c.priority}</td>
                <td className="py-3 pe-6">
                  <button
                    type="button"
                    onClick={() => handleToggleEnabled(c)}
                    className={`inline-flex rounded-full px-2 py-0.5 text-2xs font-semibold uppercase tracking-wider ${
                      c.enabled
                        ? "bg-success-surface text-success-foreground"
                        : "bg-surface-inset text-foreground-muted"
                    }`}
                  >
                    {c.enabled ? t("table.on") : t("table.off")}
                  </button>
                </td>
                <td className="py-3 pe-6 text-end">
                  <div className="flex items-center justify-end gap-1">
                    <button
                      type="button"
                      onClick={() => handleTestConfig(c)}
                      disabled={testingId === c.id}
                      className="rounded px-2 py-1 text-xs text-info-foreground hover:bg-info-surface disabled:opacity-50"
                    >
                      {testingId === c.id ? t("table.testing") : t("table.test")}
                    </button>
                    <button
                      type="button"
                      onClick={() => openEditConfig(c)}
                      className="rounded px-2 py-1 text-xs text-foreground-muted hover:bg-surface-inset hover:text-foreground"
                    >
                      {t("table.edit")}
                    </button>
                    <button
                      type="button"
                      onClick={() => handleDeleteConfig(c.id)}
                      disabled={deleteConfig.isPending}
                      className="rounded px-2 py-1 text-xs text-danger-foreground hover:bg-danger-surface disabled:opacity-50"
                    >
                      {deleteConfig.isPending
                        ? t("table.deleting")
                        : t("table.delete")}
                    </button>
                  </div>
                </td>
              </tr>
            ))}
            {configs.length === 0 && (
              <tr>
                <td
                  colSpan={6}
                  className="py-8 text-center text-foreground-muted"
                >
                  {t("table.empty")}
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </div>

      <div className="mt-12 flex items-center justify-between">
        <div>
          <Heading level={2} size={4}>
            {t("rules.heading")}
          </Heading>
          <p className="mt-1 text-sm text-foreground-muted">
            {t("rules.subtitle")}
          </p>
        </div>
        {!ruleFormOpen && configs.length > 0 && (
          <Button variant="primary" size="sm" onClick={openCreateRule}>
            {t("rules.addAction")}
          </Button>
        )}
      </div>

      {ruleFormOpen && (
        <RuleForm
          form={ruleForm}
          setForm={setRuleForm}
          configs={configs}
          operations={operations}
          isEditing={!!editingRuleId}
          saving={savingRule}
          onSubmit={handleSubmitRule}
          onCancel={cancelRuleForm}
        />
      )}

      <div
        className="-mx-6 mt-6 overflow-x-auto px-6"
        tabIndex={0}
        role="region"
        aria-label={t("rules.regionLabel")}
      >
        <table className="w-full min-w-[640px] text-sm">
          <thead>
            <tr className="border-b border-divider text-start text-xs font-medium uppercase text-foreground-muted">
              <th className="py-3 pe-6">{t("rules.operation")}</th>
              <th className="py-3 pe-6">{t("rules.modelConfig")}</th>
              <th className="py-3 pe-6">{t("rules.priority")}</th>
              <th className="py-3 pe-6">{t("rules.enabled")}</th>
              <th className="py-3 pe-6 text-end">{t("rules.actions")}</th>
            </tr>
          </thead>
          <tbody>
            {rules.map((r) => (
              <tr key={r.id} className="border-b border-divider-soft">
                <td className="py-3 pe-6 font-medium text-foreground-strong">
                  {r.operation}
                </td>
                <td className="py-3 pe-6 text-foreground-muted">
                  {configName(r.model_config_id)}
                </td>
                <td className="py-3 pe-6 text-foreground-muted">{r.priority}</td>
                <td className="py-3 pe-6">
                  <span
                    className={`inline-flex rounded-full px-2 py-0.5 text-2xs font-semibold uppercase tracking-wider ${
                      r.enabled
                        ? "bg-success-surface text-success-foreground"
                        : "bg-surface-inset text-foreground-muted"
                    }`}
                  >
                    {r.enabled ? t("table.on") : t("table.off")}
                  </span>
                </td>
                <td className="py-3 pe-6 text-end">
                  <div className="flex items-center justify-end gap-1">
                    <button
                      type="button"
                      onClick={() => openEditRule(r)}
                      className="rounded px-2 py-1 text-xs text-foreground-muted hover:bg-surface-inset hover:text-foreground"
                    >
                      {t("rules.edit")}
                    </button>
                    <button
                      type="button"
                      onClick={() => handleDeleteRule(r.id)}
                      disabled={deleteRule.isPending}
                      className="rounded px-2 py-1 text-xs text-danger-foreground hover:bg-danger-surface disabled:opacity-50"
                    >
                      {deleteRule.isPending
                        ? t("rules.deleting")
                        : t("rules.delete")}
                    </button>
                  </div>
                </td>
              </tr>
            ))}
            {rules.length === 0 && (
              <tr>
                <td
                  colSpan={5}
                  className="py-8 text-center text-foreground-muted"
                >
                  {t("rules.empty")}
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </div>
    </SettingsPageShell>
  );
}

// ---------------------------------------------------------------------------
// Provider badge — semantic tone per provider family
// ---------------------------------------------------------------------------

function ProviderBadge({ provider }: { provider: string }) {
  const tone =
    provider === "anthropic" || provider === "openai"
      ? "bg-info-surface text-info-foreground"
      : provider === "bedrock" || provider === "vertex"
        ? "bg-warning-surface text-warning-foreground"
        : "bg-surface-inset text-foreground-muted";

  return (
    <span
      className={`inline-flex rounded-full px-2 py-0.5 text-2xs font-semibold uppercase tracking-wider ${tone}`}
    >
      {provider}
    </span>
  );
}

// ---------------------------------------------------------------------------
// Config form
// ---------------------------------------------------------------------------

function ConfigForm({
  form,
  setForm,
  errors,
  clearError,
  isEditing,
  saving,
  onSubmit,
  onCancel,
}: {
  form: ConfigFormValues;
  setForm: React.Dispatch<React.SetStateAction<ConfigFormValues>>;
  errors: Record<string, string>;
  clearError: (field: string) => void;
  isEditing: boolean;
  saving: boolean;
  onSubmit: (e: React.FormEvent) => void;
  onCancel: () => void;
}) {
  const t = useTranslations("settings.runtime.models.form.config");
  const tCommon = useTranslations("common");
  const update = (field: string, patch: Partial<ConfigFormValues>) => {
    setForm((prev) => ({ ...prev, ...patch }));
    clearError(field);
  };

  return (
    <form
      onSubmit={onSubmit}
      className="mt-4 rounded-lg border border-brand-border bg-brand-surface p-4"
    >
      <div className="mb-3 flex items-center justify-between">
        <span className="text-xs font-semibold text-brand-foreground">
          {isEditing ? t("editTitle") : t("createTitle")}
        </span>
        <button
          type="button"
          onClick={onCancel}
          className="text-xs text-foreground-muted hover:text-foreground"
        >
          {tCommon("cancel")}
        </button>
      </div>

      <div className="grid grid-cols-2 gap-3">
        <Field
          className="col-span-2"
          label={t("name")}
          error={errors.name}
        >
          <FormInput
            value={form.name}
            onChange={(e) => update("name", { name: e.target.value })}
            placeholder={t("namePlaceholder")}
            required
            error={!!(errors.name)} className="mt-0.5 text-xs"
          />
        </Field>

        <Field label={t("provider")} error={errors.provider}>
          <SettingsSelect
            label={t("provider")}
            hideLabel
            value={form.provider}
            onChange={(e) => update("provider", { provider: e.target.value })}
            className={errors.provider ? "border-danger-border" : ""}
          >
            {PROVIDERS.map((p) => (
              <option key={p} value={p}>
                {p}
              </option>
            ))}
          </SettingsSelect>
        </Field>

        <Field label={t("modelId")} error={errors.model_id}>
          <FormInput
            value={form.model_id}
            onChange={(e) => update("model_id", { model_id: e.target.value })}
            placeholder={t("modelIdPlaceholder")}
            required
            error={!!(errors.model_id)} className="mt-0.5 text-xs"
          />
        </Field>

        <Field label={t("maxTokens")} error={errors.max_tokens}>
          <FormInput
            type="number"
            min={1}
            value={form.max_tokens}
            onChange={(e) =>
              update("max_tokens", { max_tokens: Number(e.target.value) })
            }
            error={!!(errors.max_tokens)} className="mt-0.5 text-xs"
          />
        </Field>

        <Field
          label={
            <>
              {t("temperature")}{" "}
              <span className="normal-case text-foreground-muted">
                {t("optional")}
              </span>
            </>
          }
          error={errors.temperature}
        >
          <FormInput
            type="number"
            min={0}
            max={2}
            step={0.1}
            value={form.temperature}
            onChange={(e) =>
              update("temperature", { temperature: e.target.value })
            }
            placeholder={t("temperaturePlaceholder")}
            error={!!(errors.temperature)} className="mt-0.5 text-xs"
          />
        </Field>

        <Field label={t("timeoutSecs")}>
          <FormInput
            type="number"
            min={1}
            value={form.timeout_secs}
            onChange={(e) =>
              update("timeout_secs", { timeout_secs: Number(e.target.value) })
            }
 className="mt-0.5 text-xs"
          />
        </Field>

        <Field label={t("priority")}>
          <FormInput
            type="number"
            value={form.priority}
            onChange={(e) =>
              update("priority", { priority: Number(e.target.value) })
            }
 className="mt-0.5 text-xs"
          />
        </Field>

        <Field
          label={
            <>
              {t("apiKeyEnv")}{" "}
              <span className="normal-case text-foreground-muted">
                {t("optional")}
              </span>
            </>
          }
        >
          <FormInput
            value={form.api_key_env}
            onChange={(e) =>
              update("api_key_env", { api_key_env: e.target.value })
            }
            placeholder={t("apiKeyEnvPlaceholder")}
 className="mt-0.5 text-xs"
          />
        </Field>

        <Field
          label={
            <>
              {t("baseUrl")}{" "}
              <span className="normal-case text-foreground-muted">
                {t("optional")}
              </span>
            </>
          }
        >
          <FormInput
            value={form.base_url}
            onChange={(e) =>
              update("base_url", { base_url: e.target.value })
            }
            placeholder={t("baseUrlPlaceholder")}
 className="mt-0.5 text-xs"
          />
        </Field>

        <Field
          label={
            <>
              {t("region")}{" "}
              <span className="normal-case text-foreground-muted">
                {t("optional")}
              </span>
            </>
          }
        >
          <FormInput
            value={form.region}
            onChange={(e) => update("region", { region: e.target.value })}
            placeholder={t("regionPlaceholder")}
 className="mt-0.5 text-xs"
          />
        </Field>

        <Field
          label={
            <>
              {t("costPerInput")}{" "}
              <span className="normal-case text-foreground-muted">
                {t("usd")}
              </span>
            </>
          }
        >
          <FormInput
            type="number"
            step="0.01"
            min="0"
            value={form.cost_per_1m_input}
            onChange={(e) =>
              update("cost_per_1m_input", {
                cost_per_1m_input: e.target.value,
              })
            }
            placeholder={t("costPerInputPlaceholder")}
 className="mt-0.5 text-xs"
          />
        </Field>

        <Field
          label={
            <>
              {t("costPerOutput")}{" "}
              <span className="normal-case text-foreground-muted">
                {t("usd")}
              </span>
            </>
          }
        >
          <FormInput
            type="number"
            step="0.01"
            min="0"
            value={form.cost_per_1m_output}
            onChange={(e) =>
              update("cost_per_1m_output", {
                cost_per_1m_output: e.target.value,
              })
            }
            placeholder={t("costPerOutputPlaceholder")}
 className="mt-0.5 text-xs"
          />
        </Field>

        <Field
          label={
            <>
              {t("dailyBudget")}{" "}
              <span className="normal-case text-foreground-muted">
                {t("usd")}
              </span>
            </>
          }
        >
          <FormInput
            type="number"
            step="0.01"
            min="0"
            value={form.daily_budget_usd}
            onChange={(e) =>
              update("daily_budget_usd", {
                daily_budget_usd: e.target.value,
              })
            }
            placeholder={t("dailyBudgetPlaceholder")}
 className="mt-0.5 text-xs"
          />
        </Field>

        <div className="flex items-center self-end pb-1">
          <SettingsSwitch
            label={t("enabled")}
            checked={form.enabled}
            onChange={(v) => update("enabled", { enabled: v })}
          />
        </div>
      </div>

      <div className="mt-3 flex items-center gap-2">
        <Button
          type="submit"
          variant="primary"
          size="sm"
          disabled={!form.name.trim() || !form.model_id.trim() || saving}
        >
          {saving
            ? isEditing
              ? t("updating")
              : t("creating")
            : isEditing
              ? t("updateSubmit")
              : t("createSubmit")}
        </Button>
        <button
          type="button"
          onClick={onCancel}
          className="rounded-md px-3 py-1.5 text-xs text-foreground-muted hover:bg-surface-inset"
        >
          {tCommon("cancel")}
        </button>
      </div>
    </form>
  );
}

// ---------------------------------------------------------------------------
// Rule form
// ---------------------------------------------------------------------------

function RuleForm({
  form,
  setForm,
  configs,
  operations,
  isEditing,
  saving,
  onSubmit,
  onCancel,
}: {
  form: RuleFormValues;
  setForm: React.Dispatch<React.SetStateAction<RuleFormValues>>;
  configs: ModelConfig[];
  operations: string[];
  isEditing: boolean;
  saving: boolean;
  onSubmit: (e: React.FormEvent) => void;
  onCancel: () => void;
}) {
  const t = useTranslations("settings.runtime.models.form.rule");
  const tCommon = useTranslations("common");
  const update = (patch: Partial<RuleFormValues>) =>
    setForm((prev) => ({ ...prev, ...patch }));

  return (
    <form
      onSubmit={onSubmit}
      className="mt-4 rounded-lg border border-info-border bg-info-surface p-4"
    >
      <div className="mb-3 flex items-center justify-between">
        <span className="text-xs font-semibold text-info-foreground">
          {isEditing ? t("editTitle") : t("createTitle")}
        </span>
        <button
          type="button"
          onClick={onCancel}
          className="text-xs text-foreground-muted hover:text-foreground"
        >
          {tCommon("cancel")}
        </button>
      </div>

      <div className="grid grid-cols-2 gap-3">
        <Field label={t("operation")}>
          <FormInput
            aria-label={t("operation")}
            list="model-routing-operations"
            pattern="\*|[a-z][a-z0-9_.:-]{0,127}"
            required
            value={form.operation}
            onChange={(e) => update({ operation: e.target.value })}
            density="settings"
            className="mt-0.5 font-mono"
          />
          <datalist id="model-routing-operations">
            {operations.map((o) => (
              <option key={o} value={o}>
                {o}
              </option>
            ))}
          </datalist>
        </Field>

        <Field label={t("modelConfig")}>
          <SettingsSelect
            label={t("modelConfig")}
            hideLabel
            value={form.model_config_id}
            onChange={(e) => update({ model_config_id: e.target.value })}
          >
            {configs.map((c) => (
              <option key={c.id} value={c.id}>
                {c.name} ({c.model_id})
              </option>
            ))}
          </SettingsSelect>
        </Field>

        <Field label={t("priority")}>
          <FormInput
            type="number"
            value={form.priority}
            onChange={(e) => update({ priority: Number(e.target.value) })}
 className="mt-0.5 text-xs"
          />
        </Field>

        <div className="flex items-center self-end pb-1">
          <SettingsSwitch
            label={t("enabled")}
            checked={form.enabled}
            onChange={(v) => update({ enabled: v })}
          />
        </div>
      </div>

      <div className="mt-3 flex items-center gap-2">
        <Button
          type="submit"
          variant="primary"
          size="sm"
          disabled={!form.model_config_id || saving}
        >
          {saving
            ? isEditing
              ? t("updating")
              : t("creating")
            : isEditing
              ? t("updateSubmit")
              : t("createSubmit")}
        </Button>
        <button
          type="button"
          onClick={onCancel}
          className="rounded-md px-3 py-1.5 text-xs text-foreground-muted hover:bg-surface-inset"
        >
          {tCommon("cancel")}
        </button>
      </div>
    </form>
  );
}

// ---------------------------------------------------------------------------
// Field shell — keeps label/input/error layout consistent
// ---------------------------------------------------------------------------

function Field({
  label,
  error,
  children,
  className,
}: {
  label: React.ReactNode;
  error?: string;
  children: React.ReactNode;
  className?: string;
}) {
  return (
    <div className={className}>
      <label className="text-2xs font-semibold uppercase tracking-wider text-foreground-muted">
        {label}
      </label>
      {children}
      {error && (
        <p className="mt-0.5 text-2xs text-danger-foreground">{error}</p>
      )}
    </div>
  );
}
