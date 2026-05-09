//! Map [`LlmProviderConfig`] to a built [`BrainChatModel`] and its
//! source-of-truth [`CredentialProvider`].
//!
//! Single point where ontosyx encodes "provider string → (codec,
//! transport, credential) tuple". Adding a new vendor is one match
//! arm here plus a feature flag if it's behind a cloud crate gate.
//!
//! Async because Bedrock's `BedrockCredentialProvider::default_chain`
//! eagerly resolves the AWS credential chain on construction.

use std::sync::Arc;

use entelix::auth::{ApiKeyProvider, CredentialProvider};
use entelix::codecs::AnthropicMessagesCodec;
use entelix::transports::DirectTransport;
use entelix::{ChatModel, ClaudeCodeOAuthProvider, FileCredentialStore};

#[cfg(feature = "llm-aws")]
use entelix::codecs::BedrockConverseCodec;
#[cfg(feature = "llm-aws")]
use entelix::{BedrockAuth, BedrockCredentialProvider, BedrockTransport};

use ox_core::error::{OxError, OxResult};

use crate::auth::LlmProviderConfig;
use crate::dyn_chat_model::{BrainChatModel, BrainChatModelImpl};

/// Output of [`build_chat_model`] — the dispatch facade plus the
/// underlying credential provider. The provider is returned alongside
/// because downstream agent surfaces (Phase 4 ox-agent migration) take
/// `Arc<dyn CredentialProvider>` directly when wiring auth onto an
/// `entelix::Agent` rather than going back through the chat model.
pub struct BuiltChatModel {
    pub chat_model: Arc<dyn BrainChatModel>,
    /// `None` for vendors whose auth is internal to the transport
    /// (today: Bedrock — SigV4 signs requests inside `BedrockTransport`
    /// and never surfaces an `Arc<dyn CredentialProvider>`).
    pub credentials: Option<Arc<dyn CredentialProvider>>,
}

/// Resolve a provider config into a built `BrainChatModel`.
pub async fn build_chat_model(config: &LlmProviderConfig) -> OxResult<BuiltChatModel> {
    match config.provider.as_str() {
        "anthropic" => build_anthropic(config),
        "claude-code" => build_claude_code(config),
        #[cfg(feature = "llm-aws")]
        "bedrock" => build_bedrock(config).await,
        #[cfg(not(feature = "llm-aws"))]
        "bedrock" => Err(OxError::Runtime {
            message: "bedrock provider requires the `llm-aws` feature".into(),
        }),
        other => Err(OxError::Runtime {
            message: format!(
                "Unsupported LLM provider: '{other}' (supported: anthropic, claude-code, bedrock)"
            ),
        }),
    }
}

fn build_anthropic(config: &LlmProviderConfig) -> OxResult<BuiltChatModel> {
    let api_key = config
        .api_key
        .clone()
        .or_else(|| std::env::var("ANTHROPIC_API_KEY").ok())
        .ok_or_else(|| OxError::Runtime {
            message:
                "anthropic provider: api_key missing and ANTHROPIC_API_KEY env var unset".into(),
        })?;
    let creds: Arc<dyn CredentialProvider> = Arc::new(ApiKeyProvider::anthropic(api_key));
    let transport = DirectTransport::anthropic(Arc::clone(&creds))
        .map_err(|e| OxError::Runtime { message: format!("DirectTransport build: {e}") })?;
    let model = ChatModel::new(AnthropicMessagesCodec::new(), transport, &config.model);
    Ok(BuiltChatModel {
        chat_model: Arc::new(BrainChatModelImpl::new(model, "anthropic", &config.model)),
        credentials: Some(creds),
    })
}

fn build_claude_code(config: &LlmProviderConfig) -> OxResult<BuiltChatModel> {
    let path = FileCredentialStore::default_claude_path()
        .map_err(|e| OxError::Runtime { message: format!("Claude Code credential path: {e}") })?;
    let store = FileCredentialStore::with_path(path);
    let creds: Arc<dyn CredentialProvider> = Arc::new(ClaudeCodeOAuthProvider::new(store));
    let transport = DirectTransport::anthropic(Arc::clone(&creds))
        .map_err(|e| OxError::Runtime { message: format!("DirectTransport build: {e}") })?;
    let model = ChatModel::new(AnthropicMessagesCodec::new(), transport, &config.model);
    Ok(BuiltChatModel {
        chat_model: Arc::new(BrainChatModelImpl::new(model, "claude-code", &config.model)),
        credentials: Some(creds),
    })
}

#[cfg(feature = "llm-aws")]
async fn build_bedrock(config: &LlmProviderConfig) -> OxResult<BuiltChatModel> {
    let region = config.region.clone().unwrap_or_else(|| "us-east-1".into());
    let provider = BedrockCredentialProvider::default_chain().await;
    let transport = BedrockTransport::builder()
        .with_region(region)
        .with_auth(BedrockAuth::SigV4 { provider })
        .build()
        .map_err(|e| OxError::Runtime { message: format!("BedrockTransport build: {e}") })?;
    let model = ChatModel::new(BedrockConverseCodec::new(), transport, &config.model);
    Ok(BuiltChatModel {
        chat_model: Arc::new(BrainChatModelImpl::new(model, "bedrock", &config.model)),
        credentials: None,
    })
}
