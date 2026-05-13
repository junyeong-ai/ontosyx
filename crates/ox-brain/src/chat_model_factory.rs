//! Factory: build a vendor-erased [`ChatRunnable`] from a
//! [`LlmProviderConfig`].
//!
//! One match arm per supported provider — adding a vendor is **one
//! arm here, no other ox-brain edits**. The factory emits a single
//! uniform [`ChatRunnable`] regardless of vendor; the heterogeneity
//! lives behind the `Arc<dyn DynChatModel>` inside the handle.
//!
//! Every constructed `ChatModel` rides a canonical layer stack:
//! `entelix::RetryLayer` (transient retries) is innermost,
//! `entelix::PolicyLayer` (`RunBudget` pre-call gates, per-tenant
//! cost ledger, optional PII redaction) is outermost when a
//! [`PolicyRegistry`] is wired. The order ensures a retried call
//! observes a single pre-check + single post-call charge, and a
//! `PolicyRegistry::mutate_fallback` reseed reaches the next
//! dispatch.

use std::sync::Arc;

use entelix::ChatModel;
#[cfg(feature = "llm-aws")]
use entelix::bedrock::{BedrockAuth, BedrockCredentialProvider, BedrockTransport};
#[cfg(feature = "llm-aws")]
use entelix::codecs::BedrockConverseCodec;
use entelix::transports::{RetryLayer, RetryPolicy};
use entelix::{PolicyLayer, PolicyRegistry};

use ox_core::error::{OxError, OxResult};

use crate::auth::LlmProviderConfig;
use crate::chat_model::ChatRunnable;
use crate::entelix_error::map_entelix_err;

/// Provider names supported by the factory. Operators (config files,
/// admin UIs) author against this set; unsupported names produce a
/// typed `Runtime` error rather than a silent fall-through.
pub const SUPPORTED_PROVIDERS: &[&str] = &[
    "anthropic",
    "openai",
    "gemini",
    #[cfg(feature = "llm-aws")]
    "bedrock",
];

/// Build a [`ChatRunnable`] for the given provider config. Each
/// variant resolves its own credential channel through entelix's
/// `CredentialProvider` surface — the factory never reads inline
/// API keys for vendors that prefer ambient credentials (e.g.
/// Bedrock SigV4).
///
/// `policy` is optional — production wires the workspace-wide
/// `PolicyRegistry` so the cost meter + RunBudget pre-call gates
/// apply, tests pass `None` for a pass-through dispatch.
pub async fn build_chat_model(
    config: &LlmProviderConfig,
    policy: Option<&Arc<PolicyRegistry>>,
) -> OxResult<ChatRunnable> {
    match config.provider.as_str() {
        "anthropic" => build_anthropic(config, policy),
        "openai" => build_openai(config, policy),
        "gemini" => build_gemini(config, policy),
        #[cfg(feature = "llm-aws")]
        "bedrock" => build_bedrock(config, policy).await,
        other => Err(OxError::Runtime {
            message: format!(
                "Unsupported LLM provider: '{other}'. Supported: {}",
                SUPPORTED_PROVIDERS.join(", ")
            ),
        }),
    }
}

fn require_api_key(config: &LlmProviderConfig) -> OxResult<&str> {
    config.api_key.as_deref().ok_or_else(|| OxError::Runtime {
        message: format!(
            "Provider '{}' requires an api_key — none configured",
            config.provider
        ),
    })
}

fn build_anthropic(
    config: &LlmProviderConfig,
    policy: Option<&Arc<PolicyRegistry>>,
) -> OxResult<ChatRunnable> {
    let key = require_api_key(config)?.to_owned();
    let model = ChatModel::anthropic(key, &config.model)
        .map_err(map_entelix_err("entelix chat model build failed"))?;
    let model = apply_layers(model, policy, &config.provider, &config.model);
    Ok(ChatRunnable::new(model))
}

fn build_openai(
    config: &LlmProviderConfig,
    policy: Option<&Arc<PolicyRegistry>>,
) -> OxResult<ChatRunnable> {
    let key = require_api_key(config)?.to_owned();
    let model = ChatModel::openai(key, &config.model)
        .map_err(map_entelix_err("entelix chat model build failed"))?;
    let model = apply_layers(model, policy, &config.provider, &config.model);
    Ok(ChatRunnable::new(model))
}

fn build_gemini(
    config: &LlmProviderConfig,
    policy: Option<&Arc<PolicyRegistry>>,
) -> OxResult<ChatRunnable> {
    let key = require_api_key(config)?.to_owned();
    let model = ChatModel::gemini(key, &config.model)
        .map_err(map_entelix_err("entelix chat model build failed"))?;
    let model = apply_layers(model, policy, &config.provider, &config.model);
    Ok(ChatRunnable::new(model))
}

#[cfg(feature = "llm-aws")]
async fn build_bedrock(
    config: &LlmProviderConfig,
    policy: Option<&Arc<PolicyRegistry>>,
) -> OxResult<ChatRunnable> {
    let region = config
        .region
        .clone()
        .unwrap_or_else(|| "us-east-1".to_owned());
    let provider = BedrockCredentialProvider::default_chain().await;
    let transport = BedrockTransport::builder()
        .with_region(region)
        .with_auth(BedrockAuth::SigV4 { provider })
        .build()
        .map_err(map_entelix_err("entelix chat model build failed"))?;
    let model = ChatModel::new(BedrockConverseCodec::new(), transport, &config.model);
    let model = apply_layers(model, policy, &config.provider, &config.model);
    Ok(ChatRunnable::new(model))
}

/// Compose the canonical layer stack for every `ChatModel` minted
/// through the factory.
///
/// Layer ordering (innermost → outermost, last-registered =
/// outermost per entelix `ChatModel::layer` contract):
/// 1. [`RetryLayer`] (innermost) — exponential-backoff retries on
///    transient provider errors (network / 5xx / 429). Wraps the
///    leaf dispatch so a retried call observes a single
///    [`PolicyLayer`] pre-check + single post-call charge.
/// 2. [`PolicyLayer`] (outermost) — `RunBudget` pre-call cost +
///    token gates, per-tenant cost ledger, optional PII redaction.
///    Wraps the retry chain so [`crate::ChatModelRegistry`]'s
///    `mutate_fallback` reseeds take effect on the next dispatch
///    even if retries are in flight.
///
/// `RetryLayer` rides every model regardless of policy wiring —
/// transient resilience is a baseline operator contract, not an
/// opt-in. `PolicyLayer` is opt-in via `policy.is_some()` so test
/// harnesses without a registry stay lightweight.
///
/// The resolved layer stack lands at `info` via
/// `ChatModel::layer_names()` so operator dashboards see what
/// cross-cutting layers ride every dispatch without spelunking
/// source.
fn apply_layers<C, T>(
    model: ChatModel<C, T>,
    policy: Option<&Arc<PolicyRegistry>>,
    provider: &str,
    model_id: &str,
) -> ChatModel<C, T>
where
    C: entelix::codecs::Codec + 'static,
    T: entelix::transports::Transport + 'static,
{
    let model = model.layer(RetryLayer::new(RetryPolicy::standard()));
    let model = match policy {
        Some(registry) => model.layer(PolicyLayer::new(Arc::clone(registry))),
        None => model,
    };
    tracing::info!(
        provider,
        model = model_id,
        layers = ?model.layer_names(),
        "chat model built with layer stack"
    );
    model
}
