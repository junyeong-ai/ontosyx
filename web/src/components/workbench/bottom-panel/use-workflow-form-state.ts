import { useEffect, useState } from "react";

import { emptyImportValue, type SourceImportValue } from "@/components/workbench/source-import-panel";
import type { DesignSource, LoadPlan } from "@/types/api";

export function useWorkflowFormState(projectId: string | undefined, projectTitle: string | null, sourceSchemaName: string | undefined) {
  // ---------------------------------------------------------------------------
  // Design
  // ---------------------------------------------------------------------------
  const [designContext, setDesignContext] = useState("");

  // ---------------------------------------------------------------------------
  // Complete
  // ---------------------------------------------------------------------------
  const [completeName, setCompleteName] = useState("");
  const [deployOnComplete, setDeployOnComplete] = useState(false);

  // ---------------------------------------------------------------------------
  // Deploy
  // ---------------------------------------------------------------------------
  const [deployPreview, setDeployPreview] = useState<string[] | null>(null);
  const [loadPlan, setLoadPlan] = useState<LoadPlan | null>(null);

  // ---------------------------------------------------------------------------
  // Reanalyze
  // ---------------------------------------------------------------------------
  const [showReanalyze, setShowReanalyze] = useState(false);
  const [reanalyzeConnectionString, setReanalyzeConnectionString] = useState("");
  const [reanalyzeSchemaName, setReanalyzeSchemaName] = useState(
    sourceSchemaName ?? "public",
  );
  const [reanalyzeSampleData, setReanalyzeSampleData] = useState("");
  const [reanalyzeRepoPath, setReanalyzeRepoPath] = useState("");
  const [reanalyzeRepoUrl, setReanalyzeRepoUrl] = useState("");

  // ---------------------------------------------------------------------------
  // Extend
  // ---------------------------------------------------------------------------
  const [showExtend, setShowExtend] = useState(false);
  const [extendSourceType, setExtendSourceType] = useState<DesignSource["type"]>("text");
  const [extendConnectionString, setExtendConnectionString] = useState("");
  const [extendSchemaName, setExtendSchemaName] = useState("public");
  const [extendSampleData, setExtendSampleData] = useState("");
  const [extendRepoUrl, setExtendRepoUrl] = useState("");
  const [extendDatabase, setExtendDatabase] = useState("");
  const [extendDuckdbFilePath, setExtendDuckdbFilePath] = useState("");
  // Subset/extend selection — `mode: "all"` lowers to a full sweep
  // on the new source; `mode: "subset"` lowers to an Extend
  // selection so the project absorbs only the picked tables.
  const [extendImport, setExtendImport] = useState<SourceImportValue>(
    emptyImportValue(),
  );

  // ---------------------------------------------------------------------------
  // Reset transient state when switching projects
  // ---------------------------------------------------------------------------
  useEffect(() => {
    setDeployPreview(null);
    setDeployOnComplete(false);
    setLoadPlan(null);
    setCompleteName(projectTitle ?? "");
    setShowReanalyze(false);
    setShowExtend(false);
    setExtendImport(emptyImportValue());
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [projectId]);

  return {
    design: { designContext, setDesignContext },
    complete: { completeName, setCompleteName, deployOnComplete, setDeployOnComplete },
    deploy: { deployPreview, setDeployPreview, loadPlan, setLoadPlan },
    reanalyze: {
      showReanalyze, setShowReanalyze,
      connectionString: reanalyzeConnectionString, setConnectionString: setReanalyzeConnectionString,
      schemaName: reanalyzeSchemaName, setSchemaName: setReanalyzeSchemaName,
      sampleData: reanalyzeSampleData, setSampleData: setReanalyzeSampleData,
      repoPath: reanalyzeRepoPath, setRepoPath: setReanalyzeRepoPath,
      repoUrl: reanalyzeRepoUrl, setRepoUrl: setReanalyzeRepoUrl,
    },
    extend: {
      showExtend, setShowExtend,
      sourceType: extendSourceType, setSourceType: setExtendSourceType,
      connectionString: extendConnectionString, setConnectionString: setExtendConnectionString,
      schemaName: extendSchemaName, setSchemaName: setExtendSchemaName,
      database: extendDatabase, setDatabase: setExtendDatabase,
      sampleData: extendSampleData, setSampleData: setExtendSampleData,
      repoUrl: extendRepoUrl, setRepoUrl: setExtendRepoUrl,
      duckdbFilePath: extendDuckdbFilePath, setDuckdbFilePath: setExtendDuckdbFilePath,
      importValue: extendImport, setImportValue: setExtendImport,
    },
  };
}
