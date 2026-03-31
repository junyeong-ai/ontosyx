"use client";

import { useEffect, useState } from "react";
import { useAppStore } from "@/lib/store";
import { Popover, PopoverTrigger, PopoverContent } from "@/components/ui/popover";
import { Spinner } from "@/components/ui/spinner";
import { HugeiconsIcon } from "@hugeicons/react";
import {
  FolderOpenIcon,
  ArrowDown01Icon,
  PlusSignIcon,
  DashboardSpeed01Icon,
  Message01Icon,
  Search01Icon,
} from "@hugeicons/core-free-icons";
import { toast } from "sonner";
import { useGuardPendingEdits } from "@/lib/guard-pending-edits";
import type { OntologyIR, DesignProjectSummary, Dashboard } from "@/types/api";
import {
  listProjects,
  getProject,
  createProject,
  listDashboards,
  createDashboard,
  listOntologies,
} from "@/lib/api";

// ---------------------------------------------------------------------------
// Shared trigger styling — all selectors use this exact visual wrapper
// ---------------------------------------------------------------------------

const TRIGGER_CLASS =
  "flex min-w-0 items-center gap-1.5 rounded-md border border-zinc-200 bg-zinc-50 px-2.5 py-1.5 text-xs font-medium text-zinc-700 transition-colors hover:bg-zinc-100 dark:border-zinc-700 dark:bg-zinc-900 dark:text-zinc-300 dark:hover:bg-zinc-800";

const POPOVER_CLASS =
  "z-50 w-80 rounded-lg border border-zinc-200 bg-white shadow-lg data-[starting-style]:scale-95 data-[starting-style]:opacity-0 data-[ending-style]:scale-95 data-[ending-style]:opacity-0 transition-all dark:border-zinc-700 dark:bg-zinc-900";

// ---------------------------------------------------------------------------
// ContextSelector — renders mode-appropriate selector in the header
// ---------------------------------------------------------------------------

export function ContextSelector() {
  const workspaceMode = useAppStore((s) => s.workspaceMode);

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
  const activeProject = useAppStore((s) => s.activeProject);
  const setActiveProject = useAppStore((s) => s.setActiveProject);
  const ontology = useAppStore((s) => s.ontology);
  const setOntology = useAppStore((s) => s.setOntology);
  const setDesignBottomTab = useAppStore((s) => s.setDesignBottomTab);
  const bottomPanelOpen = useAppStore((s) => s.isBottomPanelOpen);
  const toggleBottomPanel = useAppStore((s) => s.toggleBottomPanel);
  const guardPendingEdits = useGuardPendingEdits();

  const [open, setOpen] = useState(false);
  const [projects, setProjects] = useState<DesignProjectSummary[]>([]);
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    if (!open) return;
    setLoading(true);
    listProjects()
      .then((page) => setProjects(page.items))
      .catch(() => toast.error("Failed to load projects"))
      .finally(() => setLoading(false));
  }, [open]);

  const handleSelect = async (id: string) => {
    if (!(await guardPendingEdits("Switch Project"))) return;
    setOpen(false);
    try {
      const project = await getProject(id);
      setActiveProject(project);
      if (project.ontology) {
        setOntology(project.ontology as OntologyIR);
      } else {
        useAppStore.getState().resetOntology();
      }
    } catch (err) {
      console.error("Failed to load project:", err);
      toast.error("Failed to load project");
    }
  };

  // Design mode: show project title only (not standalone ontology name)
  const label = activeProject?.title || "No project";

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger className={TRIGGER_CLASS}>
        <HugeiconsIcon icon={FolderOpenIcon} className="h-3.5 w-3.5" size="100%" />
        <span className="max-w-[280px] truncate">{label}</span>
        <HugeiconsIcon icon={ArrowDown01Icon} className="h-3 w-3 text-zinc-400" size="100%" />
      </PopoverTrigger>
      <PopoverContent className={POPOVER_CLASS}>
        <div className="max-h-60 overflow-auto p-1">
          <button
            onClick={async () => {
              if (!(await guardPendingEdits("New Project"))) return;
              setOpen(false);
              setActiveProject(null);
              useAppStore.getState().resetOntology();
              setDesignBottomTab("workflow");
              if (!bottomPanelOpen) toggleBottomPanel();
            }}
            className="flex w-full items-center gap-2 rounded-md px-3 py-1.5 text-left text-xs font-medium text-indigo-600 hover:bg-indigo-50 dark:text-indigo-400 dark:hover:bg-indigo-950"
          >
            <HugeiconsIcon icon={PlusSignIcon} className="h-3 w-3" size="100%" />
            New Project
          </button>
          <div className="my-1 h-px bg-zinc-200 dark:bg-zinc-700" />
          {loading ? (
            <div className="flex items-center justify-center py-4">
              <Spinner size="sm" className="text-zinc-400" />
            </div>
          ) : projects.length === 0 ? (
            <p className="px-3 py-4 text-center text-xs text-zinc-400">No projects</p>
          ) : (
            projects.map((p) => (
              <div key={p.id} className="flex items-center gap-1">
                <button
                  onClick={() => handleSelect(p.id)}
                  className="flex flex-1 items-center gap-2 rounded-md px-3 py-1.5 text-left text-xs hover:bg-zinc-50 dark:hover:bg-zinc-800"
                >
                  <span className="flex-1 truncate text-zinc-700 dark:text-zinc-300">
                    {p.title || p.id.slice(0, 8)}
                  </span>
                  <span className="rounded bg-zinc-100 px-1 text-[9px] text-zinc-500 dark:bg-zinc-800">
                    {p.status}
                  </span>
                </button>
                {p.saved_ontology_id && (
                  <button
                    title="Fork -- create new project from this ontology"
                    aria-label="Fork project"
                    onClick={async (e) => {
                      e.stopPropagation();
                      if (!(await guardPendingEdits("Fork Project"))) return;
                      setOpen(false);
                      try {
                        const forked = await createProject({
                          origin_type: "base_ontology",
                          base_ontology_id: p.saved_ontology_id!,
                          title: `${p.title || "Untitled"} (fork)`,
                        });
                        setActiveProject(forked);
                        if (forked.ontology) setOntology(forked.ontology as OntologyIR);
                        setDesignBottomTab("workflow");
                        if (!bottomPanelOpen) toggleBottomPanel();
                        toast.success("Project forked", { description: `From: ${p.title}` });
                      } catch (err) {
                        toast.error("Fork failed", {
                          description: err instanceof Error ? err.message : "Unknown error",
                        });
                      }
                    }}
                    className="shrink-0 rounded p-1 text-zinc-400 hover:bg-zinc-100 hover:text-indigo-600 dark:hover:bg-zinc-800 dark:hover:text-indigo-400"
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
  const ontology = useAppStore((s) => s.ontology);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState(false);

  // Auto-load latest saved ontology when entering Analyze mode
  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(false);
    listOntologies({ limit: 1 })
      .then((page) => {
        if (cancelled) return;
        if (page.items.length > 0) {
          const saved = page.items[0];
          const store = useAppStore.getState();
          store.loadSavedOntology(saved.ontology_ir as OntologyIR);
          store.setSavedOntologyId(saved.id);
        }
      })
      .catch(() => {
        if (!cancelled) setError(true);
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => { cancelled = true; };
  }, []);  

  if (loading) {
    return (
      <div className={TRIGGER_CLASS}>
        <Spinner size="xs" />
        <span className="text-zinc-400">Loading ontology...</span>
      </div>
    );
  }

  if (!ontology) {
    return (
      <div className={TRIGGER_CLASS}>
        <HugeiconsIcon icon={Message01Icon} className="h-3.5 w-3.5 text-zinc-400" size="100%" />
        <span className="text-zinc-400">
          {error ? "Failed to load ontology" : "No saved ontology"}
        </span>
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
  const ontology = useAppStore((s) => s.ontology);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState(false);

  // Auto-load latest saved ontology when entering Explore mode
  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(false);
    listOntologies({ limit: 1 })
      .then((page) => {
        if (cancelled) return;
        if (page.items.length > 0) {
          const saved = page.items[0];
          const store = useAppStore.getState();
          store.loadSavedOntology(saved.ontology_ir as OntologyIR);
          store.setSavedOntologyId(saved.id);
        }
      })
      .catch(() => {
        if (!cancelled) setError(true);
      })
      .finally(() => { if (!cancelled) setLoading(false); });
    return () => { cancelled = true; };
  }, []);  

  if (loading) {
    return (
      <div className={TRIGGER_CLASS}>
        <Spinner size="xs" />
        <span className="text-zinc-400">Loading ontology...</span>
      </div>
    );
  }

  return (
    <div className={TRIGGER_CLASS}>
      <HugeiconsIcon icon={Search01Icon} className="h-3.5 w-3.5" size="100%" />
      <span className="max-w-[280px] truncate">
        {ontology?.name || (error ? "Failed to load ontology" : "No saved ontology")}
      </span>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Dashboard mode: Dashboard selector (moved from dashboard-layout toolbar)
// ---------------------------------------------------------------------------

function DashboardSelector() {
  const activeDashboardId = useAppStore((s) => s.activeDashboardId);
  const setActiveDashboardId = useAppStore((s) => s.setActiveDashboardId);

  const [open, setOpen] = useState(false);
  const [dashboards, setDashboards] = useState<Dashboard[]>([]);
  const [loading, setLoading] = useState(false);
  const [isCreateOpen, setIsCreateOpen] = useState(false);
  const [newName, setNewName] = useState("");

  // Load dashboards on mount (for label display) and when popover opens (for fresh data)
  useEffect(() => {
    listDashboards({ limit: 50 })
      .then((page) => setDashboards(page.items))
      .catch(() => {});
  }, []);

  useEffect(() => {
    if (!open) return;
    setLoading(true);
    listDashboards({ limit: 50 })
      .then((page) => setDashboards(page.items))
      .catch(() => toast.error("Failed to load dashboards"))
      .finally(() => setLoading(false));
  }, [open]);

  const handleSelect = (id: string) => {
    setActiveDashboardId(id);
    setOpen(false);
  };

  const handleCreate = async () => {
    const name = newName.trim();
    if (!name) return;
    try {
      const dash = await createDashboard({ name });
      setDashboards((prev) => [dash, ...prev]);
      setActiveDashboardId(dash.id);
      setIsCreateOpen(false);
      setNewName("");
      setOpen(false);
      toast.success("Dashboard created");
    } catch {
      toast.error("Failed to create dashboard");
    }
  };

  const activeDashboard = dashboards.find((d) => d.id === activeDashboardId);
  const label = activeDashboard?.name || "Select dashboard";

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger className={TRIGGER_CLASS}>
        <HugeiconsIcon icon={DashboardSpeed01Icon} className="h-3.5 w-3.5" size="100%" />
        <span className="max-w-[280px] truncate">{label}</span>
        <HugeiconsIcon icon={ArrowDown01Icon} className="h-3 w-3 text-zinc-400" size="100%" />
      </PopoverTrigger>
      <PopoverContent className={POPOVER_CLASS}>
        <div className="max-h-60 overflow-auto p-1">
          {isCreateOpen ? (
            <div className="space-y-2 p-2">
              <input
                value={newName}
                onChange={(e) => setNewName(e.target.value)}
                autoFocus
                placeholder="Dashboard name"
                className="w-full rounded-md border border-zinc-200 bg-white px-2.5 py-1.5 text-xs text-zinc-700 focus:border-emerald-400 focus:ring-1 focus:ring-emerald-400/50 focus:outline-none dark:border-zinc-700 dark:bg-zinc-800 dark:text-zinc-300"
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
                  onClick={() => {
                    setIsCreateOpen(false);
                    setNewName("");
                  }}
                  className="rounded-md px-2.5 py-1 text-[11px] text-zinc-500 hover:bg-zinc-100 dark:hover:bg-zinc-800"
                >
                  Cancel
                </button>
                <button
                  onClick={handleCreate}
                  disabled={!newName.trim()}
                  className="rounded-md bg-emerald-600 px-2.5 py-1 text-[11px] font-medium text-white hover:bg-emerald-700 disabled:opacity-50"
                >
                  Create
                </button>
              </div>
            </div>
          ) : (
            <>
              <button
                onClick={() => setIsCreateOpen(true)}
                className="flex w-full items-center gap-2 rounded-md px-3 py-1.5 text-left text-xs font-medium text-indigo-600 hover:bg-indigo-50 dark:text-indigo-400 dark:hover:bg-indigo-950"
              >
                <HugeiconsIcon icon={PlusSignIcon} className="h-3 w-3" size="100%" />
                New Dashboard
              </button>
              <div className="my-1 h-px bg-zinc-200 dark:bg-zinc-700" />
              {loading ? (
                <div className="flex items-center justify-center py-4">
                  <Spinner size="sm" className="text-zinc-400" />
                </div>
              ) : dashboards.length === 0 ? (
                <p className="px-3 py-4 text-center text-xs text-zinc-400">No dashboards</p>
              ) : (
                dashboards.map((d) => (
                  <button
                    key={d.id}
                    onClick={() => handleSelect(d.id)}
                    className={`flex w-full items-center gap-2 rounded-md px-3 py-1.5 text-left text-xs hover:bg-zinc-50 dark:hover:bg-zinc-800 ${
                      d.id === activeDashboardId
                        ? "bg-emerald-50 text-emerald-700 dark:bg-emerald-950/30 dark:text-emerald-400"
                        : "text-zinc-700 dark:text-zinc-300"
                    }`}
                  >
                    <span className="flex-1 truncate">{d.name}</span>
                    {d.is_public && (
                      <span className="rounded bg-emerald-100 px-1 text-[9px] text-emerald-600 dark:bg-emerald-900/50 dark:text-emerald-400">
                        public
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
