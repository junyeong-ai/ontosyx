"use client";

import { useEffect, useState } from "react";
import { useTranslations } from "next-intl";

import { Button } from "@/components/ui/button";
import { Heading } from "@/components/ui/heading";
import { SettingsInput } from "@/components/ui/form-input";
import { toast } from "@/components/ui/toast";
import {
  useEvaluationSettings,
  useUpdateEvaluationSettings,
} from "@/hooks/api/use-evaluation";

/**
 * Workspace-scoped regression alarm policy editor.
 *
 * Threshold + min-paired-N customise the run-vs-run hybrid lift
 * regression alarm. Backend defaults (platform constants) are
 * loaded transparently when the workspace hasn't overridden;
 * the form reads the same values either way, so the operator
 * always sees the *current effective* policy.
 *
 * Validation runs server-side (`WorkspaceEvaluationSettings::validate`);
 * an invalid threshold returns a typed `validation` error envelope
 * that the toast surfaces. The form keeps an unsaved-edit dirty
 * flag so submit is gated until the operator changes something.
 */
export function RegressionPolicyForm() {
  const t = useTranslations("settings.evaluation.regressionPolicy");
  const settingsQuery = useEvaluationSettings();
  const update = useUpdateEvaluationSettings();

  const [threshold, setThreshold] = useState<string>("");
  const [minPairedN, setMinPairedN] = useState<string>("");

  // Platform defaults — mirror Rust constants in
  // `ox-store::evaluation::RETRIEVAL_LIFT_REGRESSION_*`. Used
  // when the wire payload omits a field (BE serde emits the
  // value, but the openapi-typegen marks default-bearing
  // fields as optional, so we treat absence as "use the
  // platform default").
  const PLATFORM_THRESHOLD = -0.05;
  const PLATFORM_MIN_PAIRED_N = 3;

  useEffect(() => {
    if (settingsQuery.data) {
      const t =
        settingsQuery.data.retrieval_lift_regression_threshold ?? PLATFORM_THRESHOLD;
      const n =
        settingsQuery.data.retrieval_lift_regression_min_paired_case_count ??
        PLATFORM_MIN_PAIRED_N;
      setThreshold(t.toFixed(3));
      setMinPairedN(String(n));
    }
  }, [settingsQuery.data]);

  const currentThreshold =
    settingsQuery.data?.retrieval_lift_regression_threshold ?? PLATFORM_THRESHOLD;
  const currentMinN =
    settingsQuery.data?.retrieval_lift_regression_min_paired_case_count ??
    PLATFORM_MIN_PAIRED_N;
  const dirty =
    !!settingsQuery.data &&
    (Number.parseFloat(threshold) !== currentThreshold ||
      Number.parseInt(minPairedN, 10) !== currentMinN);

  const onSubmit = () => {
    const parsedThreshold = Number.parseFloat(threshold);
    const parsedMinN = Number.parseInt(minPairedN, 10);
    if (!Number.isFinite(parsedThreshold) || !Number.isFinite(parsedMinN)) {
      toast.error(t("invalidNumber"));
      return;
    }
    update.mutate(
      {
        retrieval_lift_regression_threshold: parsedThreshold,
        retrieval_lift_regression_min_paired_case_count: parsedMinN,
      },
      {
        onSuccess: () => toast.success(t("saved")),
        onError: (err) =>
          toast.error(
            err instanceof Error ? err.message : t("saveFailed"),
          ),
      },
    );
  };

  return (
    <section className="mb-6 rounded-xl border border-divider bg-surface-base p-4">
      <Heading level={2} size={5}>
        {t("title")}
      </Heading>
      <p className="mt-1 text-xs text-foreground-muted">
        {t("description")}
      </p>
      <div className="mt-3 grid grid-cols-1 gap-3 md:grid-cols-[1fr_1fr_auto] md:items-end">
        <SettingsInput
          label={t("thresholdLabel")}
          type="number"
          step="0.001"
          min={-0.999}
          max={-0.001}
          value={threshold}
          onChange={(e) => setThreshold(e.target.value)}
          aria-describedby="threshold-help"
        />
        <SettingsInput
          label={t("minPairedNLabel")}
          type="number"
          min={2}
          step={1}
          value={minPairedN}
          onChange={(e) => setMinPairedN(e.target.value)}
          aria-describedby="min-n-help"
        />
        <Button
          type="button"
          onClick={onSubmit}
          disabled={!dirty || update.isPending}
          loading={update.isPending}
        >
          {update.isPending ? t("saving") : t("save")}
        </Button>
      </div>
      <p id="threshold-help" className="mt-2 text-2xs text-foreground-muted">
        {t("thresholdHelp")}
      </p>
      <p id="min-n-help" className="mt-1 text-2xs text-foreground-muted">
        {t("minPairedNHelp")}
      </p>
    </section>
  );
}
