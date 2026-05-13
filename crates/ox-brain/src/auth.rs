//! `LlmProviderConfig` — canonical provider configuration.
//!
//! Single source of truth for LLM provider settings. Used by:
//!
//! - `ox-api` config (deserialised from `ontosyx.toml` / env vars)
//! - `ox-brain::chat_model_factory` (vendor-specific construction)
//! - `ox-store::model_configs` (DB-backed runtime config)
//!
//! Authentication resolution lives inside
//! [`crate::chat_model_factory::build_chat_model`]; this struct is
//! pure configuration data — no credential resolution side effects.

use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub struct LlmProviderConfig {
    /// Provider name — e.g. `"anthropic"`, `"openai"`, `"gemini"`,
    /// `"bedrock"` (with `llm-aws` feature), `"claude-code"` (with
    /// `auth-claude-code` feature). See
    /// [`crate::chat_model_factory::SUPPORTED_PROVIDERS`].
    pub provider: String,
    /// Vendor model identifier — e.g. `"claude-opus-4-7"`,
    /// `"gpt-5"`, `"gemini-3.1-pro-preview"`. Used verbatim in the
    /// outbound request.
    pub model: String,
    /// Inline API key. Required for `anthropic` / `openai` / `gemini`;
    /// ignored by `bedrock` (uses AWS credential chain) and
    /// `claude-code` (uses OAuth credential file).
    #[serde(default)]
    pub api_key: Option<String>,
    /// Optional base URL override — useful for proxies or
    /// vendor-compatible endpoints.
    #[serde(default)]
    pub base_url: Option<String>,
    /// Region — required for `bedrock`. Defaults to `us-east-1`
    /// inside the factory when absent.
    #[serde(default)]
    pub region: Option<String>,
    /// Per-request timeout override. `None` defers to entelix's
    /// transport default.
    #[serde(default)]
    pub timeout_secs: Option<u64>,
}

impl LlmProviderConfig {
    /// Convenience constructor for an Anthropic config with an inline
    /// API key. Tests and example code call this; production paths
    /// resolve via DB-backed model configs.
    #[must_use]
    pub fn anthropic(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            provider: "anthropic".to_owned(),
            model: model.into(),
            api_key: Some(api_key.into()),
            base_url: None,
            region: None,
            timeout_secs: None,
        }
    }

    /// Convenience constructor for a Bedrock config bound to a region.
    /// Credentials come from the AWS credential chain at build time.
    #[cfg(feature = "llm-aws")]
    #[must_use]
    pub fn bedrock(region: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            provider: "bedrock".to_owned(),
            model: model.into(),
            api_key: None,
            base_url: None,
            region: Some(region.into()),
            timeout_secs: None,
        }
    }
}

impl fmt::Debug for LlmProviderConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LlmProviderConfig")
            .field("provider", &self.provider)
            .field("model", &self.model)
            .field("api_key", &self.api_key.as_ref().map(|_| "[REDACTED]"))
            .field("base_url", &self.base_url)
            .field("region", &self.region)
            .field("timeout_secs", &self.timeout_secs)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_redacts_api_key() {
        let config = LlmProviderConfig::anthropic("sk-secret-value", "claude-opus-4-7");
        let rendered = format!("{config:?}");
        assert!(!rendered.contains("sk-secret-value"));
        assert!(rendered.contains("[REDACTED]"));
    }

    #[test]
    fn anthropic_constructor_carries_inline_key() {
        let config = LlmProviderConfig::anthropic("sk-test", "claude-opus-4-7");
        assert_eq!(config.provider, "anthropic");
        assert_eq!(config.model, "claude-opus-4-7");
        assert_eq!(config.api_key.as_deref(), Some("sk-test"));
    }

    #[cfg(feature = "llm-aws")]
    #[test]
    fn bedrock_constructor_carries_region() {
        let config = LlmProviderConfig::bedrock("us-east-1", "anthropic.claude-opus-4-7");
        assert_eq!(config.provider, "bedrock");
        assert_eq!(config.region.as_deref(), Some("us-east-1"));
        assert!(config.api_key.is_none());
    }
}
