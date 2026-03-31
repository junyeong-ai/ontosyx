import { useEffect, useState } from "react";
import type { DesignSource, LoadPlan } from "@/types/api";

export function useWorkflowFormState(projectId: string | undefined, projectTitle: string | null, sourceSchemaName: string | undefined) {
  // ---------------------------------------------------------------------------
  // Design
  // ---------------------------------------------------------------------------
  const [designContext, setDesignContext] = useState("");
  const [acknowledgeLargeSchema, setAcknowledgeLargeSchema] = useState(false);

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
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [projectId]);

  return {
    design: { designContext, setDesignContext, acknowledgeLargeSchema, setAcknowledgeLargeSchema },
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
    },
  };
}
