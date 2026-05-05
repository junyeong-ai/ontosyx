"use client";

import { useEffect, useState } from "react";
import { useRouter } from "next/navigation";
import { useTranslations } from "next-intl";
import { useAppStore } from "@/lib/store";
import { useWorkspaceMode } from "@/hooks/use-workspace-mode";
import { Popover, PopoverTrigger, PopoverContent } from "@/components/ui/popover";
import { Spinner } from "@/components/ui/spinner";
import { FormInput } from "@/components/ui/form-input";
import { HugeiconsIcon } from "@hugeicons/react";
import {
  FolderOpenIcon,
  ArrowDown01Icon,
  PlusSignIcon,
  DashboardSpeed01Icon,
  Message01Icon,
  Search01Icon,
} from "@hugeicons/core-free-icons";
import { toast } from "@/components/ui/toast";
import { Button } from "@/components/ui/button";
import { useGuardPendingEdits } from "@/lib/guard-pending-edits";
import type { OntologyIR } from "@/types/api";
import { getProject, createProject } from "@/lib/api";
import { useProjects } from "@/hooks/api/use-projects";
import { useCreateDashboard, useDashboards } from "@/hooks/api/use-dashboards";
import { useWorkspaceOntology } from "@/hooks/api/use-workspace-ontology";

// ---------------------------------------------------------------------------
// Shared trigger styling — all selectors use this exact visual wrapper
// ---------------------------------------------------------------------------

const TRIGGER_CLASS =
  "flex min-w-0 items-center gap-1.5 rounded-md border border-divider bg-surface-raised px-2.5 py-1.5 text-xs font-medium text-foreground transition-colors duration-[var(--duration-quick)] ease-[var(--ease-out)] hover:bg-surface-inset";

const POPOVER_CLASS =
  "z-popover w-80 rounded-lg border border-divider bg-surface-base shadow-3 data-[starting-style]:scale-95 data-[starting-style]:opacity-0 data-[ending-style]:scale-95 data-[ending-style]:opacity-0 transition-all duration-[var(--duration-base)] ease-[var(--ease-out)]";

// ---------------------------------------------------------------------------
// ContextSelector — renders mode-appropriate selector in the header
// ---------------------------------------------------------------------------

export function ContextSelector() {
  const workspaceMode = useWorkspaceMode();

  switch (workspaceMode) {
    case "design":
      return <DesignSelector />;
    case "analyze":
      return <AnalyzeSelector />;
    case "explore":
      return <ExploreSelector />;
    case "dashboard":
      return <DashboardSelector />;
    default:
      return null;
  }
}

// ---------------------------------------------------------------------------
// Design mode: Project selector
// ---------------------------------------------------------------------------

function DesignSelector() {
  const t = useTranslations("chrome.contextSelector");
  const activeProject = useAppStore((s) => s.activeProject);
  const applyProjectSnapshot = useAppStore((s) => s.applyProjectSnapshot);
  const setDesignBottomTab = useAppStore((s) => s.setDesignBottomTab);
  const bottomPanelOpen = useAppStore((s) => s.isBottomPanelOpen);
  const toggleBottomPanel = useAppStore((s) => s.toggleBottomPanel);
  const guardPendingEdits = useGuardPendingEdits();

  const [open, setOpen] = useState(false);

  // Why: only fetch projects while the popover is open — `enabled` gates the
  // query so closed selectors don't consume bandwidth.
  const { data, isFetching, isError } = useProjects(undefined, { enabled: open });
  const projects = data?.items ?? [];

  useEffect(() => {
    if (isError) toast.error(t("toast.loadProjectsFailed"));
  }, [isError, t]);

  const handleSelect = async (id: string) => {
    if (!(await guardPendingEdits(t("guardSwitchProject")))) return;
    setOpen(false);
    try {
      const project = await getProject(id);
      applyProjectSnapshot(project);
    } catch (err) {
      console.error("Failed to load project:", err);
      toast.error(t("toast.loadProjectFailed"));
    }
  };

  // Design mode: show project title only (not standalone ontology name)
  const label = activeProject?.title || t("noProject");

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger className={TRIGGER_CLASS}>
        <HugeiconsIcon icon={FolderOpenIcon} className="h-3.5 w-3.5" size="100%" />
        <span className="max-w-[280px] truncate">{label}</span>
        <HugeiconsIcon icon={ArrowDown01Icon} className="h-3 w-3 text-foreground-muted" size="100%" />
      </PopoverTrigger>
      <PopoverContent className={POPOVER_CLASS}>
        <div className="max-h-60 overflow-auto p-1">
          <button
            type="button"
            onClick={async () => {
              if (!(await guardPendingEdits(t("guardNewProject")))) return;
              setOpen(false);
              applyProjectSnapshot(null);
              setDesignBottomTab("workflow");
              if (!bottomPanelOpen) toggleBottomPanel();
            }}
            className="flex w-full items-center gap-2 rounded-md px-3 py-1.5 text-start text-xs font-medium text-concept-foreground hover:bg-concept-surface"
          >
            <HugeiconsIcon icon={PlusSignIcon} className="h-3 w-3" size="100%" />
            {t("newProject")}
          </button>
          <div className="my-1 h-px bg-surface-inset" />
          {isFetching ? (
            <div className="flex items-center justify-center py-4">
              <Spinner size="sm" className="text-foreground-muted" />
            </div>
          ) : projects.length === 0 ? (
            <p className="px-3 py-4 text-center text-xs text-foreground-muted">{t("noProjects")}</p>
          ) : (
            projects.map((p) => (
              <div key={p.id} className="flex items-center gap-1">
                <button
                  type="button"
                  onClick={() => handleSelect(p.id)}
                  className="flex flex-1 items-center gap-2 rounded-md px-3 py-1.5 text-start text-xs hover:bg-surface-raised"
                >
                  <span className="flex-1 truncate text-foreground">
                    {p.title || p.id.slice(0, 8)}
                  </span>
                  <span className="rounded bg-surface-inset px-1 text-2xs text-foreground-muted">
                    {p.status}
                  </span>
                </button>
                {p.ontology_id && (
                  <button
                    type="button"
                    title={t("forkTitle")}
                    aria-label={t("forkAria")}
                    onClick={async (e) => {
                      e.stopPropagation();
                      if (!(await guardPendingEdits(t("guardForkProject")))) return;
                      setOpen(false);
                      try {
                        const forked = await createProject({
                          origin_type: "base_ontology",
                          base_ontology_id: p.ontology_id!,
                          title: `${p.title || t("untitledProject")} (fork)`,
                        });
                        applyProjectSnapshot(forked);
                        setDesignBottomTab("workflow");
                        if (!bottomPanelOpen) toggleBottomPanel();
                        toast.success(t("toast.forked"), { description: t("forkedDescription", { title: p.title ?? t("untitledProject") }) });
                      } catch (err) {
                        toast.error(t("toast.forkFailed"), {
                          description: err instanceof Error ? err.message : t("toast.unknownError"),
                        });
                      }
                    }}
                    className="shrink-0 rounded p-1 text-foreground-muted hover:bg-surface-inset hover:text-concept-foreground"
                  >
                    <HugeiconsIcon icon={PlusSignIcon} className="h-3 w-3" size="100%" />
                  </button>
                )}
              </div>
            ))
          )}
        </div>
      </PopoverContent>
    </Popover>
  );
}

// ---------------------------------------------------------------------------
// Analyze mode: Auto-loads latest saved ontology on mount
// ---------------------------------------------------------------------------

function AnalyzeSelector() {
  const t = useTranslations("chrome.contextSelector");
  const ontology = useAppStore((s) => s.ontology);
  const workspaceReady = useAppStore((s) => s.workspaceReady);
  const router = useRouter();

  // Auto-load workspace canonical ontology when entering Analyze
  // mode (after workspace init). The detail query already returns
  // the hydrated IR — no second fetch needed.
  const { data: detail, isFetching, isError } = useWorkspaceOntology({
    enabled: workspaceReady,
  });

  useEffect(() => {
    if (!detail?.ontology_ir) return;
    const store = useAppStore.getState();
    store.loadStandaloneOntology(detail.ontology_ir as OntologyIR);
    store.setOntologyId(detail.id);
  }, [detail]);

  const loading = isFetching;
  const error = isError;
  if (error) {
    toast.error(t("toast.loadOntologyFailed"));
  }

  if (loading) {
    return (
      <div className={TRIGGER_CLASS}>
        <Spinner size="xs" />
        <span className="text-foreground-muted">{t("loadingOntology")}</span>
      </div>
    );
  }

  if (!ontology) {
    return (
      <div className="flex items-center gap-2">
        <div className={TRIGGER_CLASS}>
          <HugeiconsIcon icon={Message01Icon} className="h-3.5 w-3.5 text-foreground-muted" size="100%" />
          <span className="text-foreground-muted">
            {error ? t("toast.loadOntologyFailed") : t("noSavedOntology")}
          </span>
        </div>
        {!error && (
          <Button
            variant="outline"
            size="xs"
            onClick={() => router.push("/design")}
          >
            {t("switchToDesign")}
          </Button>
        )}
      </div>
    );
  }

  return (
    <div className={TRIGGER_CLASS}>
      <HugeiconsIcon icon={Message01Icon} className="h-3.5 w-3.5" size="100%" />
      <span className="max-w-[280px] truncate">{ontology.name}</span>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Explore mode: Auto-loads latest saved ontology (same as Analyze)
// ---------------------------------------------------------------------------

function ExploreSelector() {
  const t = useTranslations("chrome.contextSelector");
  const ontology = useAppStore((s) => s.ontology);
  const workspaceReady = useAppStore((s) => s.workspaceReady);

  const { data: detail, isFetching, isError } = useWorkspaceOntology({
    enabled: workspaceReady,
  });

  useEffect(() => {
    if (!detail?.ontology_ir) return;
    const store = useAppStore.getState();
    store.loadStandaloneOntology(detail.ontology_ir as OntologyIR);
    store.setOntologyId(detail.id);
  }, [detail]);

  const loading = isFetching;
  const error = isError;

  if (loading) {
    return (
      <div className={TRIGGER_CLASS}>
        <Spinner size="xs" />
        <span className="text-foreground-muted">{t("loadingOntology")}</span>
      </div>
    );
  }

  return (
    <div className={TRIGGER_CLASS}>
      <HugeiconsIcon icon={Search01Icon} className="h-3.5 w-3.5" size="100%" />
      <span className="max-w-[280px] truncate">
        {ontology?.name || (error ? t("toast.loadOntologyFailed") : t("noSavedOntology"))}
      </span>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Dashboard mode: Dashboard selector (moved from dashboard-layout toolbar)
// ---------------------------------------------------------------------------

function DashboardSelector() {
  const t = useTranslations("chrome.contextSelector");
  const tCommon = useTranslations("common");
  const activeDashboardId = useAppStore((s) => s.activeDashboardId);
  const setActiveDashboardId = useAppStore((s) => s.setActiveDashboardId);

  const [open, setOpen] = useState(false);
  const [isCreateOpen, setIsCreateOpen] = useState(false);
  const [newName, setNewName] = useState("");

  // Why: TanStack shares one cache across mounts — this query covers both the
  // always-rendered label (needs the full list to resolve `activeDashboardId`)
  // and the popover. `isFetching` reflects any active refetch.
  const { data, isFetching, isError } = useDashboards({ limit: 50 });
  const dashboards = data?.items ?? [];

  useEffect(() => {
    if (open && isError) toast.error(t("toast.loadDashboardsFailed"));
  }, [open, isError, t]);

  const createMutation = useCreateDashboard();
  const loading = isFetching;

  const handleSelect = (id: string) => {
    setActiveDashboardId(id);
    setOpen(false);
  };

  const handleCreate = () => {
    const name = newName.trim();
    if (!name) return;
    createMutation.mutate(
      { name },
      {
        onSuccess: (dash) => {
          setActiveDashboardId(dash.id);
          setIsCreateOpen(false);
          setNewName("");
          setOpen(false);
          toast.success(t("toast.dashboardCreated"));
        },
        onError: () => toast.error(t("toast.dashboardCreateFailed")),
      },
    );
  };

  const activeDashboard = dashboards.find((d) => d.id === activeDashboardId);
  const label = activeDashboard?.name || t("selectDashboard");

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger className={TRIGGER_CLASS}>
        <HugeiconsIcon icon={DashboardSpeed01Icon} className="h-3.5 w-3.5" size="100%" />
        <span className="max-w-[280px] truncate">{label}</span>
        <HugeiconsIcon icon={ArrowDown01Icon} className="h-3 w-3 text-foreground-muted" size="100%" />
      </PopoverTrigger>
      <PopoverContent className={POPOVER_CLASS}>
        <div className="max-h-60 overflow-auto p-1">
          {isCreateOpen ? (
            <div className="space-y-2 p-2">
              <FormInput
                value={newName}
                onChange={(e) => setNewName(e.target.value)}
                autoFocus
                placeholder={t("dashboardNamePlaceholder")}
                density="settings"
                onKeyDown={(e) => {
                  if (e.key === "Enter") handleCreate();
                  if (e.key === "Escape") {
                    setIsCreateOpen(false);
                    setNewName("");
                  }
                }}
              />
              <div className="flex justify-end gap-1.5">
                <button
                  type="button"
                  onClick={() => {
                    setIsCreateOpen(false);
                    setNewName("");
                  }}
                  className="rounded-md px-2.5 py-1 text-2xs text-foreground-muted hover:bg-surface-inset"
                >
                  {tCommon("cancel")}
                </button>
                <button
                  type="button"
                  onClick={handleCreate}
                  disabled={!newName.trim()}
                  className="rounded-md bg-brand-solid px-2.5 py-1 text-2xs font-medium text-foreground-onbrand hover:bg-brand-solid disabled:opacity-50"
                >
                  {tCommon("create")}
                </button>
              </div>
            </div>
          ) : (
            <>
              <button
                type="button"
                onClick={() => setIsCreateOpen(true)}
                className="flex w-full items-center gap-2 rounded-md px-3 py-1.5 text-start text-xs font-medium text-concept-foreground hover:bg-concept-surface"
              >
                <HugeiconsIcon icon={PlusSignIcon} className="h-3 w-3" size="100%" />
                {t("newDashboard")}
              </button>
              <div className="my-1 h-px bg-surface-inset" />
              {loading ? (
                <div className="flex items-center justify-center py-4">
                  <Spinner size="sm" className="text-foreground-muted" />
                </div>
              ) : dashboards.length === 0 ? (
                <p className="px-3 py-4 text-center text-xs text-foreground-muted">{t("noDashboards")}</p>
              ) : (
                dashboards.map((d) => (
                  <button
                    type="button"
                    key={d.id}
                    onClick={() => handleSelect(d.id)}
                    className={`flex w-full items-center gap-2 rounded-md px-3 py-1.5 text-start text-xs hover:bg-surface-raised ${
                      d.id === activeDashboardId
                        ? "bg-brand-surface text-brand-foreground"
                        : "text-foreground"
                    }`}
                  >
                    <span className="flex-1 truncate">{d.name}</span>
                    {d.is_public && (
                      <span className="rounded bg-brand-surface-strong px-1 text-2xs text-brand-foreground-strong">
                        {t("publicBadge")}
                      </span>
                    )}
                  </button>
                ))
              )}
            </>
          )}
        </div>
      </PopoverContent>
    </Popover>
  );
}
