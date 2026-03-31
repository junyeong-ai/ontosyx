use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use ndarray::Axis;
use ort::session::Session;
use ort::value::Tensor;
use ox_core::error::{OxError, OxResult};
use serde::Deserialize;
use tokenizers::Tokenizer;
use tracing::info;

use super::{EmbeddingProvider, EmbeddingRole};

// ---------------------------------------------------------------------------
// Model configuration — auto-detected from files in the model directory
// ---------------------------------------------------------------------------

/// How to pool the per-token hidden states into a single embedding vector.
///
/// Detected from `1_Pooling/config.json` in the model directory.
/// Covers all pooling modes defined by sentence-transformers.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
enum PoolingMode {
    /// Use the [CLS] token (first token) hidden state.
    Cls,
    /// Average all token hidden states (default, used by qwen3-embedding).
    #[default]
    Mean,
    /// Element-wise max over all token hidden states.
    Max,
    /// Weighted mean: weight each token by its position index.
    WeightedMean,
    /// Mean divided by sqrt(sequence length).
    MeanSqrtLen,
    /// Take the hidden state of the last non-padding token (used by jina-v5).
    LastToken,
}

/// Runtime model configuration resolved at load time.
#[derive(Debug, Clone)]
struct ModelConfig {
    pooling: PoolingMode,
    /// Sentence-transformers prompt templates (e.g. `{"query": "Query: ", "document": "Document: "}`).
    prompts: HashMap<String, String>,
    /// Whether the ONNX model expects a `position_ids` input tensor.
    has_position_ids: bool,
}

// -- Config file schemas (for serde) --

#[derive(Deserialize, Default)]
struct PoolingFileConfig {
    #[serde(default)]
    pooling_mode_cls_token: bool,
    #[serde(default)]
    pooling_mode_mean_tokens: bool,
    #[serde(default)]
    pooling_mode_max_tokens: bool,
    #[serde(default)]
    pooling_mode_weightedmean_tokens: bool,
    #[serde(default)]
    pooling_mode_mean_sqrt_len_tokens: bool,
    #[serde(default)]
    pooling_mode_lasttoken: bool,
}

#[derive(Deserialize)]
struct SentenceTransformersFileConfig {
    #[serde(default)]
    prompts: HashMap<String, String>,
}

// ---------------------------------------------------------------------------
// OnnxEmbeddingProvider — local ONNX model for text embedding
// ---------------------------------------------------------------------------

/// Local embedding provider using an ONNX model and HuggingFace tokenizer.
///
/// Model-agnostic: auto-detects pooling mode, prompt templates, and input
/// tensors from config files in the model directory.
///
/// Supported models include:
/// - `qwen3-embedding-0.6b-onnx` (mean pooling, Instruct/Query format, position_ids)
/// - `jina-embeddings-v5-text-small-retrieval` (last-token pooling, Query/Document prefix)
/// - Any sentence-transformers compatible ONNX model
///
/// Thread-safe: `Session` is behind `Mutex` (required by ort's `&mut self` run API),
/// `Tokenizer` is behind `Arc`. CPU-bound inference is offloaded to `spawn_blocking`.
pub struct OnnxEmbeddingProvider {
    session: Arc<Mutex<Session>>,
    tokenizer: Arc<Tokenizer>,
    dimensions: usize,
    model_name: String,
    config: Arc<ModelConfig>,
}

impl OnnxEmbeddingProvider {
    /// Load an ONNX model and tokenizer from a directory.
    ///
    /// The directory should contain:
    /// - `model.onnx` (root or `onnx/` subdirectory)
    /// - `tokenizer.json`
    ///
    /// Optional config files for auto-detection:
    /// - `1_Pooling/config.json` — pooling mode
    /// - `config_sentence_transformers.json` — prompt templates
    ///
    /// Dimensions are auto-detected via a probe inference.
    pub fn load(model_dir: &Path) -> OxResult<Self> {
        // Resolve ONNX model path: check root, then onnx/ subdirectory
        let model_path = resolve_model_path(model_dir)?;

        let mut session = Session::builder()
            .and_then(|mut b| b.commit_from_file(&model_path))
            .map_err(|e| OxError::Runtime {
                message: format!("Failed to load ONNX model from {}: {e}", model_path.display()),
            })?;

        let tokenizer_path = model_dir.join("tokenizer.json");
        let tokenizer = Tokenizer::from_file(&tokenizer_path).map_err(|e| OxError::Runtime {
            message: format!(
                "Failed to load tokenizer from {}: {e}",
                tokenizer_path.display()
            ),
        })?;

        // Auto-detect model configuration from directory contents
        let has_position_ids = session.inputs().iter().any(|i| i.name() == "position_ids");
        let pooling = load_pooling_mode(model_dir);
        let prompts = load_prompt_templates(model_dir);

        let config = Arc::new(ModelConfig {
            pooling,
            prompts,
            has_position_ids,
        });

        // Detect dimensions via probe inference (reliable across ort versions)
        let probe = run_inference_inner(&mut session, &tokenizer, "dimension probe", &config)?;
        let dimensions = probe.len();

        let model_name = model_dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("onnx-embedding")
            .to_string();

        info!(
            model = %model_name,
            dimensions,
            pooling = ?config.pooling,
            has_position_ids = config.has_position_ids,
            prompts = ?config.prompts.keys().collect::<Vec<_>>(),
            path = %model_path.display(),
            "ONNX embedding model loaded"
        );

        Ok(Self {
            session: Arc::new(Mutex::new(session)),
            tokenizer: Arc::new(tokenizer),
            dimensions,
            model_name,
            config,
        })
    }

    /// Format input text with the appropriate prompt prefix or instruction format.
    ///
    /// Strategy (in priority order):
    /// 1. **Sentence-transformers prompts** (jina-v5): use `EmbeddingRole` to select
    ///    `"Query: "` or `"Document: "` prefix from `config_sentence_transformers.json`.
    /// 2. **Instruction-aware** (qwen3): `"Instruct: {instruction}\nQuery: {text}"`.
    /// 3. **Fallback**: plain text.
    fn format_input(&self, text: &str, instruction: &str, role: EmbeddingRole) -> String {
        if !self.config.prompts.is_empty() {
            let role_key = match role {
                EmbeddingRole::Query => "query",
                EmbeddingRole::Document => "document",
            };
            if let Some(prefix) = self.config.prompts.get(role_key) {
                return format!("{prefix}{text}");
            }
        }

        // Instruction-aware mode (e.g. qwen3-embedding)
        if instruction.is_empty() {
            text.to_string()
        } else {
            format!("Instruct: {instruction}\nQuery: {text}")
        }
    }
}

// ---------------------------------------------------------------------------
// Model directory resolution helpers
// ---------------------------------------------------------------------------

/// Resolve the ONNX model file path, checking multiple standard locations.
fn resolve_model_path(model_dir: &Path) -> OxResult<std::path::PathBuf> {
    let candidates = [
        model_dir.join("model_quantized.onnx"),
        model_dir.join("model.onnx"),
        model_dir.join("onnx/model_quantized.onnx"),
        model_dir.join("onnx/model.onnx"),
    ];

    for path in &candidates {
        if path.exists() {
            return Ok(path.clone());
        }
    }

    Err(OxError::Runtime {
        message: format!(
            "No ONNX model found in {}. Searched: model.onnx, model_quantized.onnx, onnx/model.onnx",
            model_dir.display()
        ),
    })
}

/// Read `1_Pooling/config.json` to determine pooling mode.
///
/// Checks all sentence-transformers pooling flags in priority order.
/// Falls back to `Mean` if no config file is found or no flag is set.
fn load_pooling_mode(model_dir: &Path) -> PoolingMode {
    let path = model_dir.join("1_Pooling/config.json");
    let contents = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return PoolingMode::default(),
    };
    let cfg: PoolingFileConfig = match serde_json::from_str(&contents) {
        Ok(c) => c,
        Err(_) => return PoolingMode::default(),
    };

    // Check flags in sentence-transformers priority order
    if cfg.pooling_mode_cls_token {
        PoolingMode::Cls
    } else if cfg.pooling_mode_lasttoken {
        PoolingMode::LastToken
    } else if cfg.pooling_mode_mean_tokens {
        PoolingMode::Mean
    } else if cfg.pooling_mode_max_tokens {
        PoolingMode::Max
    } else if cfg.pooling_mode_weightedmean_tokens {
        PoolingMode::WeightedMean
    } else if cfg.pooling_mode_mean_sqrt_len_tokens {
        PoolingMode::MeanSqrtLen
    } else {
        PoolingMode::default()
    }
}

/// Read `config_sentence_transformers.json` for prompt templates.
fn load_prompt_templates(model_dir: &Path) -> HashMap<String, String> {
    let path = model_dir.join("config_sentence_transformers.json");
    if let Ok(contents) = std::fs::read_to_string(&path) {
        if let Ok(cfg) = serde_json::from_str::<SentenceTransformersFileConfig>(&contents) {
            return cfg.prompts;
        }
    }
    HashMap::new()
}

// ---------------------------------------------------------------------------
// Pooling implementations
// ---------------------------------------------------------------------------

/// Apply pooling to reduce `[seq_len, hidden_dim]` → `[hidden_dim]`.
///
/// All modes respect the attention mask to exclude padding tokens,
/// matching the sentence-transformers reference implementation.
fn apply_pooling(
    seq_output: ndarray::ArrayView2<f32>,
    attention: &[u32],
    pooling: &PoolingMode,
) -> OxResult<ndarray::Array1<f32>> {
    let hidden_dim = seq_output.shape()[1];

    match pooling {
        PoolingMode::Cls => {
            // First token (CLS) hidden state — no mask needed.
            Ok(seq_output.index_axis(Axis(0), 0).to_owned())
        }
        PoolingMode::LastToken => {
            // Last non-padding token position.
            let last_idx = attention
                .iter()
                .rposition(|&v| v != 0)
                .unwrap_or(0);
            Ok(seq_output.index_axis(Axis(0), last_idx).to_owned())
        }
        PoolingMode::Mean => {
            // Masked mean: sum(token * mask) / sum(mask)
            let mut sum = vec![0.0f32; hidden_dim];
            let mut mask_sum = 0.0f32;
            for (row, &m) in seq_output.rows().into_iter().zip(attention.iter()) {
                let m = m as f32;
                mask_sum += m;
                for (s, v) in sum.iter_mut().zip(row.iter()) {
                    *s += v * m;
                }
            }
            if mask_sum == 0.0 {
                return Err(OxError::Runtime {
                    message: "Mean pooling failed: no valid tokens".to_string(),
                });
            }
            for s in &mut sum {
                *s /= mask_sum;
            }
            Ok(ndarray::Array1::from_vec(sum))
        }
        PoolingMode::Max => {
            // Masked max: padding positions set to -inf so they don't dominate.
            let mut result = vec![f32::NEG_INFINITY; hidden_dim];
            for (row, &m) in seq_output.rows().into_iter().zip(attention.iter()) {
                if m == 0 {
                    continue;
                }
                for (r, v) in result.iter_mut().zip(row.iter()) {
                    if *v > *r {
                        *r = *v;
                    }
                }
            }
            Ok(ndarray::Array1::from_vec(result))
        }
        PoolingMode::WeightedMean => {
            // Position-weighted masked mean: weight = position * mask.
            // Matches sentence-transformers: weights = [1, 2, 3, ...] * mask.
            let mut sum = vec![0.0f32; hidden_dim];
            let mut weight_sum = 0.0f32;
            for (i, (row, &m)) in seq_output
                .rows()
                .into_iter()
                .zip(attention.iter())
                .enumerate()
            {
                let w = (i + 1) as f32 * m as f32;
                weight_sum += w;
                for (s, v) in sum.iter_mut().zip(row.iter()) {
                    *s += v * w;
                }
            }
            if weight_sum > 0.0 {
                for s in &mut sum {
                    *s /= weight_sum;
                }
            }
            Ok(ndarray::Array1::from_vec(sum))
        }
        PoolingMode::MeanSqrtLen => {
            // sum(token * mask) / sqrt(sum(mask))
            let mut sum = vec![0.0f32; hidden_dim];
            let mut mask_sum = 0.0f32;
            for (row, &m) in seq_output.rows().into_iter().zip(attention.iter()) {
                let m = m as f32;
                mask_sum += m;
                for (s, v) in sum.iter_mut().zip(row.iter()) {
                    *s += v * m;
                }
            }
            if mask_sum == 0.0 {
                return Err(OxError::Runtime {
                    message: "MeanSqrtLen pooling failed: no valid tokens".to_string(),
                });
            }
            let sqrt_len = mask_sum.sqrt();
            for s in &mut sum {
                *s /= sqrt_len;
            }
            Ok(ndarray::Array1::from_vec(sum))
        }
    }
}

// ---------------------------------------------------------------------------
// Inference
// ---------------------------------------------------------------------------

/// Core inference logic operating on a mutable Session reference.
fn run_inference_inner(
    session: &mut Session,
    tokenizer: &Tokenizer,
    text: &str,
    config: &ModelConfig,
) -> OxResult<Vec<f32>> {
    let encoding = tokenizer.encode(text, true).map_err(|e| OxError::Runtime {
        message: format!("Tokenization failed: {e}"),
    })?;

    let ids = encoding.get_ids();
    let attention = encoding.get_attention_mask();
    let seq_len = ids.len();

    let input_ids = Tensor::<i64>::from_array((
        [1, seq_len],
        ids.iter().map(|&v| v as i64).collect::<Vec<_>>(),
    ))
    .map_err(|e| OxError::Runtime {
        message: format!("Failed to create input_ids tensor: {e}"),
    })?;

    let attention_mask = Tensor::<i64>::from_array((
        [1, seq_len],
        attention.iter().map(|&v| v as i64).collect::<Vec<_>>(),
    ))
    .map_err(|e| OxError::Runtime {
        message: format!("Failed to create attention_mask tensor: {e}"),
    })?;

    let outputs = if config.has_position_ids {
        let position_ids = Tensor::<i64>::from_array((
            [1, seq_len],
            (0..seq_len as i64).collect::<Vec<_>>(),
        ))
        .map_err(|e| OxError::Runtime {
            message: format!("Failed to create position_ids tensor: {e}"),
        })?;
        session
            .run(ort::inputs![input_ids, attention_mask, position_ids])
            .map_err(|e| OxError::Runtime {
                message: format!("ONNX inference failed: {e}"),
            })?
    } else {
        session
            .run(ort::inputs![input_ids, attention_mask])
            .map_err(|e| OxError::Runtime {
                message: format!("ONNX inference failed: {e}"),
            })?
    };

    // Extract output as dynamic-dimension ndarray view
    let output = outputs[0]
        .try_extract_array::<f32>()
        .map_err(|e| OxError::Runtime {
            message: format!("Failed to extract output tensor: {e}"),
        })?;

    let pooled = if output.ndim() == 2 {
        // Shape: [batch=1, hidden_dim] — already pooled by the model
        let view = output.index_axis(Axis(0), 0);
        view.iter().copied().collect::<Vec<f32>>()
    } else if output.ndim() == 3 {
        // Shape: [batch=1, seq_len, hidden_dim] — needs pooling
        let batch0 = output.index_axis(Axis(0), 0); // [seq_len, hidden_dim]
        let seq_output = batch0
            .into_dimensionality::<ndarray::Ix2>()
            .map_err(|e| OxError::Runtime {
                message: format!("Failed to reshape output to 2D: {e}"),
            })?;
        let pooled = apply_pooling(seq_output.view(), attention, &config.pooling)?;
        pooled.to_vec()
    } else {
        return Err(OxError::Runtime {
            message: format!("Unexpected output shape: {:?}", output.shape()),
        });
    };

    // L2 normalize
    let norm: f32 = pooled.iter().map(|v| v * v).sum::<f32>().sqrt();
    if norm > 0.0 {
        Ok(pooled.iter().map(|v| v / norm).collect())
    } else {
        Ok(pooled)
    }
}

/// Run inference through the mutex-guarded session.
fn run_inference(
    session: &Mutex<Session>,
    tokenizer: &Tokenizer,
    text: &str,
    config: &ModelConfig,
) -> OxResult<Vec<f32>> {
    let mut guard = session.lock().map_err(|e| OxError::Runtime {
        message: format!("ONNX session lock poisoned: {e}"),
    })?;
    run_inference_inner(&mut guard, tokenizer, text, config)
}

#[async_trait]
impl EmbeddingProvider for OnnxEmbeddingProvider {
    async fn embed(&self, text: &str, instruction: &str, role: EmbeddingRole) -> OxResult<Vec<f32>> {
        let input = self.format_input(text, instruction, role);
        let session = Arc::clone(&self.session);
        let tokenizer = Arc::clone(&self.tokenizer);
        let config = Arc::clone(&self.config);
        tokio::task::spawn_blocking(move || run_inference(&session, &tokenizer, &input, &config))
            .await
            .map_err(|e| OxError::Runtime {
                message: format!("Embedding task failed: {e}"),
            })?
    }

    fn dimensions(&self) -> usize {
        self.dimensions
    }

    fn provider_name(&self) -> &str {
        &self.model_name
    }
}
