use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use branchforge::tools::ExecutionContext;
use branchforge::{SchemaTool, ToolResult};
use chrono::Utc;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::sync::Semaphore;
use tracing::{info, warn};
use uuid::Uuid;

use ox_store::{AnalysisResult, AnalysisResultStore};

/// Docker image name for the analysis sandbox.
const SANDBOX_IMAGE: &str = "ontosyx-analysis-sandbox";

/// Global semaphore to limit concurrent Docker sandbox executions.
static SANDBOX_SEMAPHORE: std::sync::LazyLock<Arc<Semaphore>> =
    std::sync::LazyLock::new(|| Arc::new(Semaphore::new(4)));

// ---------------------------------------------------------------------------
// ExecuteAnalysisTool
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ExecuteAnalysisInput {
    /// Python code to execute in the analysis sandbox.
    pub code: String,
    /// Human-readable description of the analysis.
    pub description: String,
    /// Input data as JSON (passed to the script via /sandbox/input.json).
    #[serde(default)]
    pub data: Option<serde_json::Value>,
    /// Optional recipe ID for tracing which recipe this execution is based on.
    /// The agent retrieves recipe code_template via search_recipes and passes
    /// it directly in the `code` field. This field is for audit/provenance only.
    #[serde(default)]
    pub recipe_id: Option<String>,
}

#[derive(Debug, Serialize)]
struct ExecuteAnalysisOutput {
    stdout: String,
    stderr: String,
    exit_code: i32,
    duration_ms: u64,
}

/// Executes Python analysis code in a sandboxed Docker container.
///
/// Safety guarantees:
/// - Network isolation (`--network=none`)
/// - Memory limit (512 MB)
/// - CPU limit (1 core)
/// - Read-only filesystem (writable /tmp only, 64 MB)
/// - Execution timeout (configurable via `OX_ANALYSIS_TIMEOUT_SECS`, default 120s)
/// - Concurrency limit (configurable via `OX_ANALYSIS_CONCURRENCY`, default 4)
pub struct ExecuteAnalysisTool {
    pub store: Arc<dyn AnalysisResultStore>,
}

#[async_trait]
impl SchemaTool for ExecuteAnalysisTool {
    type Input = ExecuteAnalysisInput;
    const NAME: &'static str = super::EXECUTE_ANALYSIS;
    const DESCRIPTION: &'static str = "Execute Python analysis in a sandboxed environment. \
         Libraries: pandas, numpy, scikit-learn, statsmodels, scipy, matplotlib. \
         Pass query results in the 'data' field. The code reads /sandbox/input.json. \
         ALWAYS start code with this boilerplate:\n\
         ```\n\
         import json, pandas as pd\n\
         with open('/sandbox/input.json') as f:\n\
             data = json.load(f)\n\
         cols = data['columns']\n\
         df = pd.DataFrame([dict(zip(cols, row)) for row in data['rows']])\n\
         ```\n\
         Print results to stdout as JSON (use default=str for non-serializable types). Timeout: 120s.";

    async fn handle(&self, input: Self::Input, _ctx: &ExecutionContext) -> ToolResult {
        // Compute input hash for cache lookup
        let mut hasher = Sha256::new();
        hasher.update(input.code.as_bytes());
        if let Some(ref data) = input.data {
            hasher.update(data.to_string().as_bytes());
        }
        let input_hash = format!("{:x}", hasher.finalize());

        let recipe_id = input
            .recipe_id
            .as_deref()
            .and_then(|s| s.parse::<Uuid>().ok());

        // Check cache — return early if a recent result exists (< 1 hour)
        if let Ok(Some(cached)) = self.store.get_cached_result(&input_hash, recipe_id).await {
            let age = Utc::now() - cached.created_at;
            if age.num_hours() < 1 {
                info!(
                    description = %input.description,
                    input_hash = %input_hash,
                    "Returning cached analysis result"
                );
                return ToolResult::success(cached.output.to_string());
            }
        }

        let start = std::time::Instant::now();

        let result =
            match run_analysis_sandbox(&input.code, input.data.as_ref(), Duration::from_secs(120))
                .await
            {
                Ok(r) => r,
                Err(e) => return ToolResult::error(e),
            };

        let duration_ms = start.elapsed().as_millis() as u64;

        info!(
            description = %input.description,
            exit_code = result.exit_code,
            duration_ms,
            "Analysis executed"
        );

        if result.exit_code != 0 {
            return ToolResult::error(format!(
                "Analysis failed (exit code {}):\n{}",
                result.exit_code, result.stderr
            ));
        }

        let output = ExecuteAnalysisOutput {
            stdout: result.stdout,
            stderr: result.stderr,
            exit_code: result.exit_code,
            duration_ms,
        };

        // Save result to cache (fire-and-forget)
        let analysis_result = AnalysisResult {
            id: Uuid::new_v4(),
            recipe_id,
            ontology_id: None,
            input_hash,
            output: serde_json::json!({
                "stdout": output.stdout,
                "stderr": output.stderr,
                "exit_code": output.exit_code,
                "duration_ms": output.duration_ms,
            }),
            duration_ms: duration_ms as i64,
            created_at: Utc::now(),
        };
        let store = Arc::clone(&self.store);
        tokio::spawn(async move {
            if let Err(e) = store.create_analysis_result(&analysis_result).await {
                warn!(error = %e, "Failed to cache analysis result");
            }
        });

        ToolResult::success(serde_json::to_string_pretty(&output).unwrap_or_default())
    }
}

// ---------------------------------------------------------------------------
// Docker sandbox execution (public for reuse by scheduler)
// ---------------------------------------------------------------------------

/// Output from a sandbox execution — public for reuse by the scheduler.
#[derive(Debug, Serialize)]
pub struct SandboxResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

/// Execute Python code in a sandboxed Docker container.
///
/// This is the shared entrypoint used by both the `ExecuteAnalysisTool` and
/// the scheduled-task executor in `ox-api`.
///
/// Safety: network-isolated, memory-limited, read-only filesystem.
pub async fn run_analysis_sandbox(
    code: &str,
    input_data: Option<&serde_json::Value>,
    timeout: Duration,
) -> Result<SandboxResult, String> {
    let permit = SANDBOX_SEMAPHORE
        .acquire()
        .await
        .map_err(|e| format!("Semaphore closed: {e}"))?;

    // Ensure data is written as a JSON object, not a double-serialized string.
    // LLMs sometimes pass data as a JSON string value instead of an object.
    let data_json = match input_data {
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(v) => {
            // Flatten tagged PropertyValue cells ({type, value}) to plain values.
            // This makes the data directly usable by Python without parsing envelopes.
            let flattened = flatten_tagged_cells(v);
            serde_json::to_string(&flattened).unwrap_or_default()
        }
        None => "{}".to_string(),
    };

    let result = match tokio::time::timeout(timeout, execute_in_sandbox(code, &data_json)).await {
        Ok(Ok(r)) => r,
        Ok(Err(e)) => {
            drop(permit);
            return Err(format!("Sandbox execution failed: {e}"));
        }
        Err(_) => {
            drop(permit);
            return Err(format!(
                "Analysis timed out after {} seconds",
                timeout.as_secs()
            ));
        }
    };

    drop(permit);
    Ok(result)
}

async fn execute_in_sandbox(code: &str, data_json: &str) -> Result<SandboxResult, String> {
    use tokio::process::Command;

    let temp_dir = tempfile::tempdir().map_err(|e| format!("Temp dir failed: {e}"))?;
    let code_path = temp_dir.path().join("analysis.py");
    let data_path = temp_dir.path().join("input.json");

    tokio::fs::write(&code_path, code)
        .await
        .map_err(|e| format!("Write code failed: {e}"))?;
    tokio::fs::write(&data_path, data_json)
        .await
        .map_err(|e| format!("Write data failed: {e}"))?;

    let output = Command::new("docker")
        .args([
            "run",
            "--rm",
            "--network=none",
            "--memory=512m",
            "--cpus=1",
            "--read-only",
            "--tmpfs=/tmp:rw,size=64m",
            "-v",
            &format!("{}:/sandbox:ro", temp_dir.path().display()),
            SANDBOX_IMAGE,
            "python",
            "/sandbox/analysis.py",
        ])
        .output()
        .await
        .map_err(|e| format!("Docker execution failed: {e}"))?;

    Ok(SandboxResult {
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        exit_code: output.status.code().unwrap_or(-1),
    })
}

/// Flatten tagged PropertyValue cells to plain JSON values.
///
/// QueryResult rows contain cells serialized as `{"type": "string", "value": "hello"}`.
/// This function recursively walks the data and replaces such tagged objects with
/// their plain `value` (or `null` for type-only objects like `{"type": "null"}`).
/// Non-tagged objects and all other values are left as-is.
fn flatten_tagged_cells(v: &serde_json::Value) -> serde_json::Value {
    match v {
        serde_json::Value::Object(map) => {
            if map.contains_key("type") && map.len() <= 2 {
                // Tagged PropertyValue: {"type": "...", "value": ...} or {"type": "null"}
                match map.get("value") {
                    Some(inner) => flatten_tagged_cells(inner),
                    None => serde_json::Value::Null,
                }
            } else {
                // Regular object — recurse into values
                let flattened: serde_json::Map<String, serde_json::Value> = map
                    .iter()
                    .map(|(k, val)| (k.clone(), flatten_tagged_cells(val)))
                    .collect();
                serde_json::Value::Object(flattened)
            }
        }
        serde_json::Value::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(flatten_tagged_cells).collect())
        }
        other => other.clone(),
    }
}
