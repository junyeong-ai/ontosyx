use std::collections::HashMap;
use std::path::Path;

use ox_core::error::{OxError, OxResult};
use serde::Deserialize;
use tracing::{info, warn};

// ---------------------------------------------------------------------------
// PromptRegistry — DB-backed prompt template management
//
// Runtime source: PostgreSQL `prompt_templates` table (single source of truth)
// Initial seed:  TOML files in `prompts/` directory (first deployment only)
// Admin updates: via REST API (POST/PATCH /api/admin/prompts)
//
// Prompts are loaded into an in-memory cache at startup.
// To apply DB changes at runtime, restart the server.
// ---------------------------------------------------------------------------

/// A single prompt template with metadata.
#[derive(Debug, Clone, Deserialize)]
pub struct PromptTemplate {
    /// Semantic version of this prompt (for tracking/logging)
    pub version: String,
    /// Human-readable description
    pub description: String,
    /// The system prompt text (instructions for the LLM)
    pub system: String,
    /// The user message template with `{{variable}}` placeholders
    pub user_template: String,
    /// Default max_tokens for this prompt
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    /// Default temperature for this prompt
    #[serde(default)]
    pub temperature: Option<f32>,
}

fn default_max_tokens() -> u32 {
    8192
}

/// TOML file wrapper — the `[prompt]` table.
#[derive(Deserialize)]
struct PromptFile {
    prompt: PromptTemplate,
}

impl PromptTemplate {
    /// Render the user template by replacing `{{key}}` with values.
    pub fn render_user(&self, vars: &HashMap<&str, &str>) -> String {
        let mut result = self.user_template.clone();
        for (key, value) in vars {
            result = result.replace(&format!("{{{{{key}}}}}"), value);
        }
        result
    }
}

// ---------------------------------------------------------------------------
// PromptVersion — parsed semantic version for enforcement
// ---------------------------------------------------------------------------

/// Parsed semantic version (major.minor.patch).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct PromptVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl PromptVersion {
    /// Parse a "major.minor.patch" version string.
    pub fn parse(version: &str) -> OxResult<Self> {
        let parts: Vec<&str> = version.split('.').collect();
        if parts.len() != 3 {
            return Err(OxError::Validation {
                field: "version".to_string(),
                message: format!(
                    "Invalid prompt version '{}': expected major.minor.patch",
                    version
                ),
            });
        }
        let major = parts[0].parse::<u32>().map_err(|_| OxError::Validation {
            field: "version".to_string(),
            message: format!("Invalid major version in '{version}'"),
        })?;
        let minor = parts[1].parse::<u32>().map_err(|_| OxError::Validation {
            field: "version".to_string(),
            message: format!("Invalid minor version in '{version}'"),
        })?;
        let patch = parts[2].parse::<u32>().map_err(|_| OxError::Validation {
            field: "version".to_string(),
            message: format!("Invalid patch version in '{version}'"),
        })?;
        Ok(Self {
            major,
            minor,
            patch,
        })
    }
}

impl std::fmt::Display for PromptVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

// ---------------------------------------------------------------------------
// PromptVersionInfo — prompt name + parsed version for external queries
// ---------------------------------------------------------------------------

/// Summary of a loaded prompt's version info.
#[derive(Debug, Clone)]
pub struct PromptVersionInfo {
    pub name: String,
    pub version: PromptVersion,
    pub raw_version: String,
}

// ---------------------------------------------------------------------------
// PromptRegistry — loads and caches prompt templates from DB
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct PromptRegistry {
    prompts: HashMap<String, PromptTemplate>,
    /// Parsed versions keyed by prompt name.
    versions: HashMap<String, PromptVersion>,
}

impl PromptRegistry {
    /// Get a prompt template by name.
    pub fn get(&self, name: &str) -> OxResult<&PromptTemplate> {
        self.prompts.get(name).ok_or_else(|| OxError::Runtime {
            message: format!(
                "Prompt '{}' not found. Available: {:?}",
                name,
                self.prompts.keys().collect::<Vec<_>>()
            ),
        })
    }

    /// Get a prompt template by name, enforcing a minimum version requirement.
    pub fn get_checked(&self, name: &str, min_version: &str) -> OxResult<&PromptTemplate> {
        let template = self.get(name)?;

        let required = PromptVersion::parse(min_version)?;

        let loaded = self.versions.get(name).ok_or_else(|| OxError::Runtime {
            message: format!(
                "Prompt '{name}' version '{}' could not be parsed; cannot enforce minimum {min_version}",
                template.version
            ),
        })?;

        if *loaded < required {
            return Err(OxError::Runtime {
                message: format!(
                    "Prompt '{}' version {} is below minimum required {}",
                    name, loaded, required
                ),
            });
        }

        Ok(template)
    }

    /// Load prompts from DB. Seeds missing prompts from TOML on every startup.
    pub async fn load_from_db(
        store: &dyn ox_store::Store,
        toml_seed_dir: Option<&Path>,
    ) -> OxResult<Self> {
        // Seed any missing prompts from TOML (idempotent per-file)
        if let Some(dir) = toml_seed_dir
            && dir.exists()
        {
            Self::seed_from_toml(store, dir).await?;
        }

        let db_prompts = store.list_prompt_templates(true).await?;

        if db_prompts.is_empty() {
            return Err(OxError::Runtime {
                message: "No prompts in DB and no seed directory available".to_string(),
            });
        }

        Self::from_db_rows(db_prompts)
    }

    /// Seed DB from TOML files (one-time, first deployment only).
    async fn seed_from_toml(store: &dyn ox_store::Store, dir: &Path) -> OxResult<()> {
        let entries = std::fs::read_dir(dir).map_err(|e| OxError::Runtime {
            message: format!("Failed to read seed directory: {e}"),
        })?;

        for entry in entries {
            let entry = entry.map_err(|e| OxError::Runtime {
                message: format!("Failed to read directory entry: {e}"),
            })?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("toml") {
                continue;
            }
            let name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string();

            let content_str = std::fs::read_to_string(&path).map_err(|e| OxError::Runtime {
                message: format!("Failed to read {}: {e}", path.display()),
            })?;
            let file: PromptFile = toml::from_str(&content_str).map_err(|e| OxError::Runtime {
                message: format!("Failed to parse {}: {e}", path.display()),
            })?;

            let combined = format!(
                "[system]\n{}\n\n[user_template]\n{}",
                file.prompt.system, file.prompt.user_template
            );

            let row = ox_store::PromptTemplateRow {
                id: uuid::Uuid::new_v4(),
                name: name.clone(),
                version: file.prompt.version.clone(),
                content: combined,
                variables: serde_json::json!([]),
                metadata: serde_json::json!({
                    "description": file.prompt.description,
                    "max_tokens": file.prompt.max_tokens,
                    "temperature": file.prompt.temperature,
                }),
                created_by: "system".to_string(),
                created_at: chrono::Utc::now(),
                is_active: true,
            };

            if let Err(e) = store.create_prompt_template(&row).await {
                warn!(name = %name, error = %e, "Failed to seed prompt");
            } else {
                info!(name = %name, version = %file.prompt.version, "Seeded prompt from TOML");
            }
        }
        Ok(())
    }

    /// Build registry from DB rows.
    fn from_db_rows(rows: Vec<ox_store::PromptTemplateRow>) -> OxResult<Self> {
        let mut prompts = HashMap::new();
        let mut versions = HashMap::new();

        for row in rows {
            let (system, user_template) = parse_db_content(&row.content);

            let max_tokens = row
                .metadata
                .get("max_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(8192) as u32;
            let temperature = row
                .metadata
                .get("temperature")
                .and_then(|v| v.as_f64())
                .map(|v| v as f32);

            let template = PromptTemplate {
                version: row.version.clone(),
                description: row
                    .metadata
                    .get("description")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                system,
                user_template,
                max_tokens,
                temperature,
            };

            if let Ok(parsed) = PromptVersion::parse(&row.version) {
                versions.insert(row.name.clone(), parsed);
            }
            prompts.insert(row.name, template);
        }

        info!(count = prompts.len(), "Prompt registry loaded from DB");

        Ok(Self { prompts, versions })
    }

    /// List all loaded prompt names and raw version strings.
    pub fn list(&self) -> Vec<(&str, &str)> {
        self.prompts
            .iter()
            .map(|(name, tmpl)| (name.as_str(), tmpl.version.as_str()))
            .collect()
    }

    /// Get parsed version info for all loaded prompts.
    pub fn versions(&self) -> Vec<PromptVersionInfo> {
        self.versions
            .iter()
            .map(|(name, ver)| PromptVersionInfo {
                name: name.clone(),
                version: ver.clone(),
                raw_version: ver.to_string(),
            })
            .collect()
    }

    /// Get the parsed version for a specific prompt.
    pub fn get_version(&self, name: &str) -> Option<&PromptVersion> {
        self.versions.get(name)
    }
}

/// Parse DB content format: "[system]\n...\n\n[user_template]\n..."
fn parse_db_content(content: &str) -> (String, String) {
    if let Some(rest) = content.strip_prefix("[system]\n")
        && let Some(split_pos) = rest.find("\n\n[user_template]\n")
    {
        let system = &rest[..split_pos];
        let user_template = &rest[split_pos + "\n\n[user_template]\n".len()..];
        return (system.to_string(), user_template.to_string());
    }
    // Content without sections (e.g., agent_system) → treat as system prompt
    (content.to_string(), String::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_user_template() {
        let tmpl = PromptTemplate {
            version: "1.0.0".to_string(),
            description: "test".to_string(),
            system: "system prompt".to_string(),
            user_template: "Question: {{question}}\n\nOntology:\n{{ontology}}".to_string(),
            max_tokens: 4096,
            temperature: None,
        };

        let mut vars = HashMap::new();
        vars.insert("question", "Who bought products?");
        vars.insert("ontology", "{\"node_types\": []}");

        let rendered = tmpl.render_user(&vars);
        assert!(rendered.contains("Who bought products?"));
        assert!(rendered.contains("{\"node_types\": []}"));
        assert!(!rendered.contains("{{"));
    }

    #[test]
    fn test_prompt_version_parse_valid() {
        let v = PromptVersion::parse("2.1.0").unwrap();
        assert_eq!(v.major, 2);
        assert_eq!(v.minor, 1);
        assert_eq!(v.patch, 0);
        assert_eq!(v.to_string(), "2.1.0");
    }

    #[test]
    fn test_prompt_version_parse_invalid() {
        assert!(PromptVersion::parse("1.0").is_err());
        assert!(PromptVersion::parse("abc").is_err());
        assert!(PromptVersion::parse("1.2.3.4").is_err());
        assert!(PromptVersion::parse("").is_err());
    }

    #[test]
    fn test_prompt_version_ordering() {
        let v1_0_0 = PromptVersion::parse("1.0.0").unwrap();
        let v1_1_0 = PromptVersion::parse("1.1.0").unwrap();
        let v2_0_0 = PromptVersion::parse("2.0.0").unwrap();
        let v2_0_1 = PromptVersion::parse("2.0.1").unwrap();

        assert!(v1_0_0 < v1_1_0);
        assert!(v1_1_0 < v2_0_0);
        assert!(v2_0_0 < v2_0_1);
        assert!(v2_0_1 > v1_0_0);
        assert_eq!(v1_0_0, PromptVersion::parse("1.0.0").unwrap());
    }
}
