"use client";

import { useState, useEffect, useCallback } from "react";
import { toast } from "sonner";
import { request } from "@/lib/api/client";
import { Spinner } from "@/components/ui/spinner";
import type { ModelConfig, ModelRoutingRule } from "@/lib/api/models";

// ---------------------------------------------------------------------------
// Form types
// ---------------------------------------------------------------------------

type ConfigFormValues = {
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
};

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

const PROVIDERS = ["anthropic", "openai", "bedrock", "vertex", "ollama", "custom"] as const;

const OPERATIONS = [
  "*",
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
] as const;

type RuleFormValues = {
  operation: string;
  model_config_id: string;
  priority: number;
  enabled: boolean;
};

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
  const [configs, setConfigs] = useState<ModelConfig[]>([]);
  const [rules, setRules] = useState<ModelRoutingRule[]>([]);
  const [loading, setLoading] = useState(true);

  // Config form state
  const [configFormOpen, setConfigFormOpen] = useState(false);
  const [editingConfigId, setEditingConfigId] = useState<string | null>(null);
  const [configForm, setConfigForm] = useState<ConfigFormValues>(EMPTY_CONFIG_FORM);
  const [savingConfig, setSavingConfig] = useState(false);
  const [deletingConfigId, setDeletingConfigId] = useState<string | null>(null);
  const [testingId, setTestingId] = useState<string | null>(null);

  // Rule form state
  const [ruleFormOpen, setRuleFormOpen] = useState(false);
  const [editingRuleId, setEditingRuleId] = useState<string | null>(null);
  const [ruleForm, setRuleForm] = useState<RuleFormValues>(EMPTY_RULE_FORM);
  const [savingRule, setSavingRule] = useState(false);
  const [deletingRuleId, setDeletingRuleId] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      const [c, r] = await Promise.all([
        request<ModelConfig[]>("/models/configs"),
        request<ModelRoutingRule[]>("/models/routing-rules"),
      ]);
      setConfigs(c);
      setRules(r);
    } catch {
      toast.error("Failed to load model configurations");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    load();
  }, [load]);

  // ---- Config CRUD ----

  const openCreateConfig = () => {
    setEditingConfigId(null);
    setConfigForm(EMPTY_CONFIG_FORM);
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
      cost_per_1m_input: c.cost_per_1m_input !== null ? String(c.cost_per_1m_input) : "",
      cost_per_1m_output: c.cost_per_1m_output !== null ? String(c.cost_per_1m_output) : "",
      daily_budget_usd: c.daily_budget_usd !== null ? String(c.daily_budget_usd) : "",
      priority: c.priority,
      enabled: c.enabled,
      api_key_env: c.api_key_env ?? "",
      region: c.region ?? "",
      base_url: c.base_url ?? "",
    });
    setConfigFormOpen(true);
  };

  const cancelConfigForm = () => {
    setConfigFormOpen(false);
    setEditingConfigId(null);
    setConfigForm(EMPTY_CONFIG_FORM);
  };

  const handleSubmitConfig = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!configForm.name.trim() || !configForm.model_id.trim()) return;

    const body: Record<string, unknown> = {
      name: configForm.name.trim(),
      provider: configForm.provider,
      model_id: configForm.model_id.trim(),
      max_tokens: configForm.max_tokens,
      temperature: configForm.temperature ? Number(configForm.temperature) : null,
      timeout_secs: configForm.timeout_secs,
      cost_per_1m_input: configForm.cost_per_1m_input ? Number(configForm.cost_per_1m_input) : null,
      cost_per_1m_output: configForm.cost_per_1m_output ? Number(configForm.cost_per_1m_output) : null,
      daily_budget_usd: configForm.daily_budget_usd ? Number(configForm.daily_budget_usd) : null,
      priority: configForm.priority,
      enabled: configForm.enabled,
      api_key_env: configForm.api_key_env.trim() || null,
      region: configForm.region.trim() || null,
      base_url: configForm.base_url.trim() || null,
    };

    setSavingConfig(true);
    try {
      if (editingConfigId) {
        await request(`/models/configs/${editingConfigId}`, {
          method: "PATCH",
          body: JSON.stringify(body),
        });
        toast.success("Model config updated");
      } else {
        await request("/models/configs", {
          method: "POST",
          body: JSON.stringify(body),
        });
        toast.success("Model config created");
      }
      cancelConfigForm();
      await load();
    } catch {
      toast.error(editingConfigId ? "Failed to update config" : "Failed to create config");
    } finally {
      setSavingConfig(false);
    }
  };

  const handleDeleteConfig = async (id: string) => {
    setDeletingConfigId(id);
    try {
      await request(`/models/configs/${id}`, { method: "DELETE" });
      toast.success("Model config deleted");
      await load();
    } catch {
      toast.error("Failed to delete config");
    } finally {
      setDeletingConfigId(null);
    }
  };

  const handleToggleEnabled = async (c: ModelConfig) => {
    try {
      await request(`/models/configs/${c.id}`, {
        method: "PATCH",
        body: JSON.stringify({ enabled: !c.enabled }),
      });
      await load();
    } catch {
      toast.error("Failed to toggle model");
    }
  };

  const handleTestConfig = async (id: string) => {
    setTestingId(id);
    try {
      const result = await request<{ success: boolean; latency_ms: number; error: string | null }>(
        "/models/test",
        { method: "POST", body: JSON.stringify({ model_config_id: id }) },
      );
      if (result.success) {
        toast.success(`Model responded in ${result.latency_ms}ms`);
      } else {
        toast.error(`Test failed: ${result.error}`);
      }
    } catch {
      toast.error("Failed to test model");
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

    const body: Record<string, unknown> = {
      operation: ruleForm.operation,
      model_config_id: ruleForm.model_config_id,
      priority: ruleForm.priority,
      enabled: ruleForm.enabled,
    };

    setSavingRule(true);
    try {
      if (editingRuleId) {
        await request(`/models/routing-rules/${editingRuleId}`, {
          method: "PATCH",
          body: JSON.stringify(body),
        });
        toast.success("Routing rule updated");
      } else {
        await request("/models/routing-rules", {
          method: "POST",
          body: JSON.stringify(body),
        });
        toast.success("Routing rule created");
      }
      cancelRuleForm();
      await load();
    } catch {
      toast.error(editingRuleId ? "Failed to update rule" : "Failed to create rule");
    } finally {
      setSavingRule(false);
    }
  };

  const handleDeleteRule = async (id: string) => {
    setDeletingRuleId(id);
    try {
      await request(`/models/routing-rules/${id}`, { method: "DELETE" });
      toast.success("Routing rule deleted");
      await load();
    } catch {
      toast.error("Failed to delete rule");
    } finally {
      setDeletingRuleId(null);
    }
  };

  // ---- Helpers ----

  const configName = (id: string) =>
    configs.find((c) => c.id === id)?.name ?? id.slice(0, 8);

  if (loading) return <Spinner />;

  return (
    <div>
      {/* Model Configs */}
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-xl font-semibold text-zinc-900 dark:text-zinc-100">
            Model Configurations
          </h1>
          <p className="mt-1 text-sm text-zinc-500 dark:text-zinc-400">
            Configure LLM providers, models, cost limits, and test connectivity.
          </p>
        </div>
        {!configFormOpen && (
          <button
            onClick={openCreateConfig}
            className="rounded-md bg-emerald-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-emerald-700"
          >
            Add Model
          </button>
        )}
      </div>

      {/* Summary */}
      <div className="mt-6 grid grid-cols-3 gap-4">
        <div className="rounded-lg border border-emerald-200 bg-emerald-50 p-4 dark:border-emerald-900 dark:bg-emerald-950">
          <div className="text-2xl font-bold text-emerald-700 dark:text-emerald-400">
            {configs.filter((c) => c.enabled).length}
          </div>
          <div className="text-xs text-emerald-600 dark:text-emerald-500">
            Enabled
          </div>
        </div>
        <div className="rounded-lg border border-zinc-200 bg-zinc-50 p-4 dark:border-zinc-700 dark:bg-zinc-900">
          <div className="text-2xl font-bold text-zinc-700 dark:text-zinc-300">
            {configs.filter((c) => !c.enabled).length}
          </div>
          <div className="text-xs text-zinc-500">Disabled</div>
        </div>
        <div className="rounded-lg border border-blue-200 bg-blue-50 p-4 dark:border-blue-900 dark:bg-blue-950">
          <div className="text-2xl font-bold text-blue-700 dark:text-blue-400">
            {rules.length}
          </div>
          <div className="text-xs text-blue-600 dark:text-blue-500">
            Routing Rules
          </div>
        </div>
      </div>

      {/* Config inline form */}
      {configFormOpen && (
        <ConfigForm
          form={configForm}
          setForm={setConfigForm}
          isEditing={!!editingConfigId}
          saving={savingConfig}
          onSubmit={handleSubmitConfig}
          onCancel={cancelConfigForm}
        />
      )}

      {/* Configs table */}
      <div className="mt-6">
        <table className="w-full text-sm">
          <thead>
            <tr className="border-b border-zinc-200 text-left text-xs font-medium uppercase text-zinc-500 dark:border-zinc-700">
              <th className="py-2">Name</th>
              <th className="py-2">Provider</th>
              <th className="py-2">Model</th>
              <th className="py-2">Priority</th>
              <th className="py-2">Enabled</th>
              <th className="py-2 text-right">Actions</th>
            </tr>
          </thead>
          <tbody>
            {configs.map((c) => (
              <tr
                key={c.id}
                className="border-b border-zinc-100 dark:border-zinc-800"
              >
                <td className="py-2 font-medium text-zinc-900 dark:text-zinc-100">
                  {c.name}
                </td>
                <td className="py-2 text-zinc-500">
                  <ProviderBadge provider={c.provider} />
                </td>
                <td className="py-2 font-mono text-xs text-zinc-500">
                  {c.model_id}
                </td>
                <td className="py-2 text-zinc-500">{c.priority}</td>
                <td className="py-2">
                  <button
                    onClick={() => handleToggleEnabled(c)}
                    className={`inline-flex rounded-full px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wider ${
                      c.enabled
                        ? "bg-emerald-100 text-emerald-700 dark:bg-emerald-900/30 dark:text-emerald-400"
                        : "bg-zinc-100 text-zinc-500 dark:bg-zinc-800 dark:text-zinc-500"
                    }`}
                  >
                    {c.enabled ? "On" : "Off"}
                  </button>
                </td>
                <td className="py-2 text-right">
                  <div className="flex items-center justify-end gap-1">
                    <button
                      onClick={() => handleTestConfig(c.id)}
                      disabled={testingId === c.id}
                      className="rounded px-2 py-1 text-xs text-blue-500 hover:bg-blue-50 hover:text-blue-700 disabled:opacity-50 dark:hover:bg-blue-950"
                    >
                      {testingId === c.id ? "..." : "Test"}
                    </button>
                    <button
                      onClick={() => openEditConfig(c)}
                      className="rounded px-2 py-1 text-xs text-zinc-500 hover:bg-zinc-100 hover:text-zinc-700 dark:hover:bg-zinc-800 dark:hover:text-zinc-300"
                    >
                      Edit
                    </button>
                    <button
                      onClick={() => handleDeleteConfig(c.id)}
                      disabled={deletingConfigId === c.id}
                      className="rounded px-2 py-1 text-xs text-red-500 hover:bg-red-50 hover:text-red-700 disabled:opacity-50 dark:hover:bg-red-950"
                    >
                      {deletingConfigId === c.id ? "..." : "Delete"}
                    </button>
                  </div>
                </td>
              </tr>
            ))}
            {configs.length === 0 && (
              <tr>
                <td colSpan={6} className="py-8 text-center text-zinc-400">
                  No model configurations
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </div>

      {/* Routing Rules */}
      <div className="mt-12 flex items-center justify-between">
        <div>
          <h2 className="text-lg font-semibold text-zinc-900 dark:text-zinc-100">
            Routing Rules
          </h2>
          <p className="mt-1 text-sm text-zinc-500 dark:text-zinc-400">
            Map operations to specific model configurations by priority.
          </p>
        </div>
        {!ruleFormOpen && configs.length > 0 && (
          <button
            onClick={openCreateRule}
            className="rounded-md bg-emerald-600 px-3 py-1.5 text-xs font-medium text-white hover:bg-emerald-700"
          >
            Add Rule
          </button>
        )}
      </div>

      {/* Rule inline form */}
      {ruleFormOpen && (
        <RuleForm
          form={ruleForm}
          setForm={setRuleForm}
          configs={configs}
          isEditing={!!editingRuleId}
          saving={savingRule}
          onSubmit={handleSubmitRule}
          onCancel={cancelRuleForm}
        />
      )}

      {/* Rules table */}
      <div className="mt-6">
        <table className="w-full text-sm">
          <thead>
            <tr className="border-b border-zinc-200 text-left text-xs font-medium uppercase text-zinc-500 dark:border-zinc-700">
              <th className="py-2">Operation</th>
              <th className="py-2">Model Config</th>
              <th className="py-2">Priority</th>
              <th className="py-2">Enabled</th>
              <th className="py-2 text-right">Actions</th>
            </tr>
          </thead>
          <tbody>
            {rules.map((r) => (
              <tr
                key={r.id}
                className="border-b border-zinc-100 dark:border-zinc-800"
              >
                <td className="py-2 font-medium text-zinc-900 dark:text-zinc-100">
                  {r.operation}
                </td>
                <td className="py-2 text-zinc-500">{configName(r.model_config_id)}</td>
                <td className="py-2 text-zinc-500">{r.priority}</td>
                <td className="py-2">
                  <span
                    className={`inline-flex rounded-full px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wider ${
                      r.enabled
                        ? "bg-emerald-100 text-emerald-700 dark:bg-emerald-900/30 dark:text-emerald-400"
                        : "bg-zinc-100 text-zinc-500 dark:bg-zinc-800 dark:text-zinc-500"
                    }`}
                  >
                    {r.enabled ? "On" : "Off"}
                  </span>
                </td>
                <td className="py-2 text-right">
                  <div className="flex items-center justify-end gap-1">
                    <button
                      onClick={() => openEditRule(r)}
                      className="rounded px-2 py-1 text-xs text-zinc-500 hover:bg-zinc-100 hover:text-zinc-700 dark:hover:bg-zinc-800 dark:hover:text-zinc-300"
                    >
                      Edit
                    </button>
                    <button
                      onClick={() => handleDeleteRule(r.id)}
                      disabled={deletingRuleId === r.id}
                      className="rounded px-2 py-1 text-xs text-red-500 hover:bg-red-50 hover:text-red-700 disabled:opacity-50 dark:hover:bg-red-950"
                    >
                      {deletingRuleId === r.id ? "..." : "Delete"}
                    </button>
                  </div>
                </td>
              </tr>
            ))}
            {rules.length === 0 && (
              <tr>
                <td colSpan={5} className="py-8 text-center text-zinc-400">
                  No routing rules configured
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Provider badge
// ---------------------------------------------------------------------------

function ProviderBadge({ provider }: { provider: string }) {
  const color =
    provider === "anthropic"
      ? "bg-orange-100 text-orange-700 dark:bg-orange-900/30 dark:text-orange-400"
      : provider === "openai"
        ? "bg-green-100 text-green-700 dark:bg-green-900/30 dark:text-green-400"
        : provider === "bedrock"
          ? "bg-amber-100 text-amber-700 dark:bg-amber-900/30 dark:text-amber-400"
          : "bg-zinc-100 text-zinc-600 dark:bg-zinc-800 dark:text-zinc-400";

  return (
    <span
      className={`inline-flex rounded-full px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wider ${color}`}
    >
      {provider}
    </span>
  );
}

// ---------------------------------------------------------------------------
// Config form (create / edit)
// ---------------------------------------------------------------------------

function ConfigForm({
  form,
  setForm,
  isEditing,
  saving,
  onSubmit,
  onCancel,
}: {
  form: ConfigFormValues;
  setForm: React.Dispatch<React.SetStateAction<ConfigFormValues>>;
  isEditing: boolean;
  saving: boolean;
  onSubmit: (e: React.FormEvent) => void;
  onCancel: () => void;
}) {
  const update = (patch: Partial<ConfigFormValues>) =>
    setForm((prev) => ({ ...prev, ...patch }));

  return (
    <form
      onSubmit={onSubmit}
      className="mt-4 rounded-lg border border-emerald-200 bg-emerald-50/50 p-4 dark:border-emerald-800 dark:bg-emerald-950/20"
    >
      <div className="mb-3 flex items-center justify-between">
        <span className="text-xs font-semibold text-emerald-700 dark:text-emerald-400">
          {isEditing ? "Edit Model Config" : "New Model Config"}
        </span>
        <button
          type="button"
          onClick={onCancel}
          className="text-xs text-zinc-400 hover:text-zinc-600"
        >
          Cancel
        </button>
      </div>

      <div className="grid grid-cols-2 gap-3">
        {/* Name */}
        <div className="col-span-2">
          <label className="text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
            Name
          </label>
          <input
            value={form.name}
            onChange={(e) => update({ name: e.target.value })}
            placeholder="e.g. Claude 4 Sonnet"
            required
            className="mt-0.5 w-full rounded-md border border-zinc-200 bg-white px-3 py-1.5 text-xs dark:border-zinc-700 dark:bg-zinc-900"
          />
        </div>

        {/* Provider */}
        <div>
          <label className="text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
            Provider
          </label>
          <select
            value={form.provider}
            onChange={(e) => update({ provider: e.target.value })}
            className="mt-0.5 w-full rounded-md border border-zinc-200 bg-white px-3 py-1.5 text-xs dark:border-zinc-700 dark:bg-zinc-900"
          >
            {PROVIDERS.map((p) => (
              <option key={p} value={p}>
                {p}
              </option>
            ))}
          </select>
        </div>

        {/* Model ID */}
        <div>
          <label className="text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
            Model ID
          </label>
          <input
            value={form.model_id}
            onChange={(e) => update({ model_id: e.target.value })}
            placeholder="e.g. claude-sonnet-4-20250514"
            required
            className="mt-0.5 w-full rounded-md border border-zinc-200 bg-white px-3 py-1.5 text-xs dark:border-zinc-700 dark:bg-zinc-900"
          />
        </div>

        {/* Max Tokens */}
        <div>
          <label className="text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
            Max Tokens
          </label>
          <input
            type="number"
            min={1}
            value={form.max_tokens}
            onChange={(e) => update({ max_tokens: Number(e.target.value) })}
            className="mt-0.5 w-full rounded-md border border-zinc-200 bg-white px-3 py-1.5 text-xs dark:border-zinc-700 dark:bg-zinc-900"
          />
        </div>

        {/* Temperature */}
        <div>
          <label className="text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
            Temperature{" "}
            <span className="normal-case text-zinc-400">(optional)</span>
          </label>
          <input
            type="number"
            min={0}
            max={2}
            step={0.1}
            value={form.temperature}
            onChange={(e) => update({ temperature: e.target.value })}
            placeholder="default"
            className="mt-0.5 w-full rounded-md border border-zinc-200 bg-white px-3 py-1.5 text-xs dark:border-zinc-700 dark:bg-zinc-900"
          />
        </div>

        {/* Timeout */}
        <div>
          <label className="text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
            Timeout (secs)
          </label>
          <input
            type="number"
            min={1}
            value={form.timeout_secs}
            onChange={(e) => update({ timeout_secs: Number(e.target.value) })}
            className="mt-0.5 w-full rounded-md border border-zinc-200 bg-white px-3 py-1.5 text-xs dark:border-zinc-700 dark:bg-zinc-900"
          />
        </div>

        {/* Priority */}
        <div>
          <label className="text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
            Priority
          </label>
          <input
            type="number"
            value={form.priority}
            onChange={(e) => update({ priority: Number(e.target.value) })}
            className="mt-0.5 w-full rounded-md border border-zinc-200 bg-white px-3 py-1.5 text-xs dark:border-zinc-700 dark:bg-zinc-900"
          />
        </div>

        {/* API Key Env */}
        <div>
          <label className="text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
            API Key Env Var{" "}
            <span className="normal-case text-zinc-400">(optional)</span>
          </label>
          <input
            value={form.api_key_env}
            onChange={(e) => update({ api_key_env: e.target.value })}
            placeholder="e.g. ANTHROPIC_API_KEY"
            className="mt-0.5 w-full rounded-md border border-zinc-200 bg-white px-3 py-1.5 text-xs dark:border-zinc-700 dark:bg-zinc-900"
          />
        </div>

        {/* Base URL */}
        <div>
          <label className="text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
            Base URL{" "}
            <span className="normal-case text-zinc-400">(optional)</span>
          </label>
          <input
            value={form.base_url}
            onChange={(e) => update({ base_url: e.target.value })}
            placeholder="e.g. https://api.example.com"
            className="mt-0.5 w-full rounded-md border border-zinc-200 bg-white px-3 py-1.5 text-xs dark:border-zinc-700 dark:bg-zinc-900"
          />
        </div>

        {/* Region */}
        <div>
          <label className="text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
            Region{" "}
            <span className="normal-case text-zinc-400">(optional)</span>
          </label>
          <input
            value={form.region}
            onChange={(e) => update({ region: e.target.value })}
            placeholder="e.g. us-east-1"
            className="mt-0.5 w-full rounded-md border border-zinc-200 bg-white px-3 py-1.5 text-xs dark:border-zinc-700 dark:bg-zinc-900"
          />
        </div>

        {/* Cost per 1M Input */}
        <div>
          <label className="text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
            Cost / 1M Input{" "}
            <span className="normal-case text-zinc-400">(USD)</span>
          </label>
          <input
            type="number"
            step="0.01"
            min="0"
            value={form.cost_per_1m_input}
            onChange={(e) => update({ cost_per_1m_input: e.target.value })}
            placeholder="e.g. 3.00"
            className="mt-0.5 w-full rounded-md border border-zinc-200 bg-white px-3 py-1.5 text-xs dark:border-zinc-700 dark:bg-zinc-900"
          />
        </div>

        {/* Cost per 1M Output */}
        <div>
          <label className="text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
            Cost / 1M Output{" "}
            <span className="normal-case text-zinc-400">(USD)</span>
          </label>
          <input
            type="number"
            step="0.01"
            min="0"
            value={form.cost_per_1m_output}
            onChange={(e) => update({ cost_per_1m_output: e.target.value })}
            placeholder="e.g. 15.00"
            className="mt-0.5 w-full rounded-md border border-zinc-200 bg-white px-3 py-1.5 text-xs dark:border-zinc-700 dark:bg-zinc-900"
          />
        </div>

        {/* Daily Budget */}
        <div>
          <label className="text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
            Daily Budget{" "}
            <span className="normal-case text-zinc-400">(USD)</span>
          </label>
          <input
            type="number"
            step="0.01"
            min="0"
            value={form.daily_budget_usd}
            onChange={(e) => update({ daily_budget_usd: e.target.value })}
            placeholder="e.g. 50.00"
            className="mt-0.5 w-full rounded-md border border-zinc-200 bg-white px-3 py-1.5 text-xs dark:border-zinc-700 dark:bg-zinc-900"
          />
        </div>

        {/* Enabled */}
        <div className="flex items-center gap-2 self-end pb-1">
          <input
            type="checkbox"
            checked={form.enabled}
            onChange={(e) => update({ enabled: e.target.checked })}
            className="rounded border-zinc-300"
          />
          <label className="text-xs text-zinc-600 dark:text-zinc-400">
            Enabled
          </label>
        </div>
      </div>

      <div className="mt-3 flex items-center gap-2">
        <button
          type="submit"
          disabled={!form.name.trim() || !form.model_id.trim() || saving}
          className="rounded-md bg-emerald-600 px-4 py-1.5 text-xs font-medium text-white disabled:opacity-50 hover:bg-emerald-700"
        >
          {saving
            ? isEditing
              ? "Updating..."
              : "Creating..."
            : isEditing
              ? "Update Config"
              : "Create Config"}
        </button>
        <button
          type="button"
          onClick={onCancel}
          className="rounded-md px-3 py-1.5 text-xs text-zinc-500 hover:bg-zinc-100 dark:hover:bg-zinc-800"
        >
          Cancel
        </button>
      </div>
    </form>
  );
}

// ---------------------------------------------------------------------------
// Rule form (create / edit)
// ---------------------------------------------------------------------------

function RuleForm({
  form,
  setForm,
  configs,
  isEditing,
  saving,
  onSubmit,
  onCancel,
}: {
  form: RuleFormValues;
  setForm: React.Dispatch<React.SetStateAction<RuleFormValues>>;
  configs: ModelConfig[];
  isEditing: boolean;
  saving: boolean;
  onSubmit: (e: React.FormEvent) => void;
  onCancel: () => void;
}) {
  const update = (patch: Partial<RuleFormValues>) =>
    setForm((prev) => ({ ...prev, ...patch }));

  return (
    <form
      onSubmit={onSubmit}
      className="mt-4 rounded-lg border border-blue-200 bg-blue-50/50 p-4 dark:border-blue-800 dark:bg-blue-950/20"
    >
      <div className="mb-3 flex items-center justify-between">
        <span className="text-xs font-semibold text-blue-700 dark:text-blue-400">
          {isEditing ? "Edit Routing Rule" : "New Routing Rule"}
        </span>
        <button
          type="button"
          onClick={onCancel}
          className="text-xs text-zinc-400 hover:text-zinc-600"
        >
          Cancel
        </button>
      </div>

      <div className="grid grid-cols-2 gap-3">
        {/* Operation */}
        <div>
          <label className="text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
            Operation
          </label>
          <select
            value={form.operation}
            onChange={(e) => update({ operation: e.target.value })}
            className="mt-0.5 w-full rounded-md border border-zinc-200 bg-white px-3 py-1.5 text-xs dark:border-zinc-700 dark:bg-zinc-900"
          >
            {OPERATIONS.map((o) => (
              <option key={o} value={o}>
                {o}
              </option>
            ))}
          </select>
        </div>

        {/* Model Config */}
        <div>
          <label className="text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
            Model Config
          </label>
          <select
            value={form.model_config_id}
            onChange={(e) => update({ model_config_id: e.target.value })}
            className="mt-0.5 w-full rounded-md border border-zinc-200 bg-white px-3 py-1.5 text-xs dark:border-zinc-700 dark:bg-zinc-900"
          >
            {configs.map((c) => (
              <option key={c.id} value={c.id}>
                {c.name} ({c.model_id})
              </option>
            ))}
          </select>
        </div>

        {/* Priority */}
        <div>
          <label className="text-[10px] font-semibold uppercase tracking-wider text-zinc-500">
            Priority
          </label>
          <input
            type="number"
            value={form.priority}
            onChange={(e) => update({ priority: Number(e.target.value) })}
            className="mt-0.5 w-full rounded-md border border-zinc-200 bg-white px-3 py-1.5 text-xs dark:border-zinc-700 dark:bg-zinc-900"
          />
        </div>

        {/* Enabled */}
        <div className="flex items-center gap-2 self-end pb-1">
          <input
            type="checkbox"
            checked={form.enabled}
            onChange={(e) => update({ enabled: e.target.checked })}
            className="rounded border-zinc-300"
          />
          <label className="text-xs text-zinc-600 dark:text-zinc-400">
            Enabled
          </label>
        </div>
      </div>

      <div className="mt-3 flex items-center gap-2">
        <button
          type="submit"
          disabled={!form.model_config_id || saving}
          className="rounded-md bg-blue-600 px-4 py-1.5 text-xs font-medium text-white disabled:opacity-50 hover:bg-blue-700"
        >
          {saving
            ? isEditing
              ? "Updating..."
              : "Creating..."
            : isEditing
              ? "Update Rule"
              : "Create Rule"}
        </button>
        <button
          type="button"
          onClick={onCancel}
          className="rounded-md px-3 py-1.5 text-xs text-zinc-500 hover:bg-zinc-100 dark:hover:bg-zinc-800"
        >
          Cancel
        </button>
      </div>
    </form>
  );
}
