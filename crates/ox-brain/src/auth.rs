use std::fmt;

use serde::{Deserialize, Serialize};

use ox_core::error::{OxError, OxResult};

// ---------------------------------------------------------------------------
// LlmProviderConfig — canonical LLM provider configuration
// ---------------------------------------------------------------------------
// Single source of truth for LLM provider settings. Used by:
// - ox-api config (deserialized from ontosyx.toml / env vars)
// - ox-brain (auth resolution, client creation)
// - ox-store model_configs table (DB-backed runtime config)

#[derive(Clone, Serialize, Deserialize)]
pub struct LlmProviderConfig {
    pub provider: String,
    pub model: String,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub region: Option<String>,
    #[serde(default)]
    pub timeout_secs: Option<u64>,
}

impl LlmProviderConfig {
    /// Resolve branchforge Auth from provider configuration.
    pub fn resolve_auth(&self) -> OxResult<branchforge::Auth> {
        match self.provider.as_str() {
            "anthropic" => {
                let key = self.api_key.as_deref().ok_or_else(|| OxError::Runtime {
                    message: "Anthropic provider requires api_key".to_string(),
                })?;
                Ok(branchforge::Auth::api_key(key))
            }
            "bedrock" => Ok(branchforge::Auth::Bedrock {
                region: self
                    .region
                    .clone()
                    .unwrap_or_else(|| "us-east-1".to_string()),
            }),
            "claude-code" => Ok(branchforge::Auth::ClaudeCli),
            other => {
                if let Some(ref key) = self.api_key {
                    Ok(branchforge::Auth::api_key(key))
                } else {
                    Err(OxError::Runtime {
                        message: format!(
                            "Unsupported LLM provider: '{other}'. Supported: anthropic, bedrock, claude-code"
                        ),
                    })
                }
            }
        }
    }

    pub fn bedrock(region: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            provider: "bedrock".to_string(),
            model: model.into(),
            api_key: None,
            base_url: None,
            region: Some(region.into()),
            timeout_secs: None,
        }
    }

    pub fn anthropic(api_key: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            provider: "anthropic".to_string(),
            model: model.into(),
            api_key: Some(api_key.into()),
            base_url: None,
            region: None,
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
