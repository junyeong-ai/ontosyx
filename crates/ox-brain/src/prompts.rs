use std::collections::HashMap;
use std::path::Path;

use ox_core::error::{OxError, OxResult};
use serde::Deserialize;
use tracing::info;

// ---------------------------------------------------------------------------
// PromptRegistry — DB-backed prompt template management
//
// Runtime source: PostgreSQL `prompt_templates` table (single source of truth)
// Initial seed:  TOML files in `prompts/` directory
// Admin updates: via REST API (POST/PATCH /api/admin/prompts)
//
// TOML / DB precedence:
//   - No DB row for a `(name, version)` pair → seed inserts it.
//   - DB row exists with matching content → silent skip (idempotent).
//   - DB row exists with diverging content + `created_by = "system"` →
//     hard error. The TOML changed without the version bump that
//     would have made it a new row. Operator must either bump the
//     TOML version or update the DB via the admin API.
//   - DB row exists with diverging content + `created_by != "system"` →
//     silent skip. An operator-edited row outranks the seed file.
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

// `PromptVersion` lives in `ox-core` so the column type on
// `PromptTemplateRow.version` (in `ox-store`) and the registry-level
// enforcement here agree on a single parsed value. Re-exported below
// for callers that already imported `ox_brain::prompts::PromptVersion`.
pub use ox_core::PromptVersion;

// ---------------------------------------------------------------------------
// PromptVersionInfo — prompt name + parsed version for external queries
// ---------------------------------------------------------------------------

/// Summary of a loaded prompt's version info.
#[derive(Debug, Clone)]
pub struct PromptVersionInfo {
    pub name: String,
    pub version: PromptVersion,
    /// The exact string written in the DB (canonical
    /// `"major.minor.patch"` form via `PromptVersion::Display`).
    pub raw_version: String,
}

// ---------------------------------------------------------------------------
// PromptRegistry — loads and caches prompt templates from DB
// ---------------------------------------------------------------------------

/// Author tag written to `prompt_templates.created_by` by the TOML
/// seed path. The drift-detection rule treats anything else as
/// operator-managed and yields to it.
const SYSTEM_CREATOR: &str = "system";

/// Decision the seed flow makes for a single TOML file given the
/// matching DB row (if any). Pure data — `seed_from_toml` performs
/// the IO; this function isolates the precedence rule for testing.
#[derive(Debug, PartialEq, Eq)]
enum SeedAction {
    /// No row at `(name, version)` — seed it.
    Insert,
    /// Row exists with byte-identical content — idempotent skip.
    SkipMatching,
    /// Row exists with diverging content authored by an operator.
    /// Operator wins; TOML hands off.
    SkipOperator { created_by: String },
    /// Row exists with diverging content authored by `system` —
    /// silent drift. Refuse to boot until the operator either bumps
    /// the TOML version or updates the row via the admin API.
    DriftError,
}

fn decide_seed_action(
    combined_content: &str,
    existing: Option<&ox_store::PromptTemplateRow>,
) -> SeedAction {
    let Some(row) = existing else {
        return SeedAction::Insert;
    };
    if row.content == combined_content {
        return SeedAction::SkipMatching;
    }
    if row.created_by == SYSTEM_CREATOR {
        SeedAction::DriftError
    } else {
        SeedAction::SkipOperator {
            created_by: row.created_by.clone(),
        }
    }
}

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

    /// Return a prompt template by name, enforcing a minimum version requirement.
    pub fn checked_for(&self, name: &str, min_version: &str) -> OxResult<&PromptTemplate> {
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

    /// Seed DB from TOML files. Idempotent under matching content,
    /// hard-errors on silent drift (TOML edited without a version
    /// bump), and yields to operator-edited rows. See the module-
    /// header precedence table.
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

            let parsed_version = PromptVersion::parse(&file.prompt.version).map_err(|e| {
                OxError::Runtime {
                    message: format!(
                        "Prompt TOML {} version '{}' isn't valid semver: {e}",
                        path.display(),
                        file.prompt.version
                    ),
                }
            })?;

            let existing = store
                .find_prompt_template_by_name_version(&name, &parsed_version)
                .await?;

            match decide_seed_action(&combined, existing.as_ref()) {
                SeedAction::Insert => {
                    let row_id = uuid::Uuid::new_v4();
                    let row = ox_store::PromptTemplateRow {
                        id: row_id,
                        name: name.clone(),
                        version: parsed_version,
                        content: combined,
                        variables: serde_json::json!([]),
                        metadata: serde_json::json!({
                            "description": file.prompt.description,
                            "max_tokens": file.prompt.max_tokens,
                            "temperature": file.prompt.temperature,
                        }),
                        created_by: SYSTEM_CREATOR.to_string(),
                        created_at: chrono::Utc::now(),
                        is_active: true,
                        workspace_id: None,
                    };
                    store.create_prompt_template(&row).await?;
                    // The "at most one active row per name" invariant is the
                    // producer's responsibility. Mirroring the admin path
                    // (`POST /api/admin/prompts`) keeps the loader's
                    // highest-version dedupe as defence in depth, not the
                    // sole gate.
                    store
                        .update_prompt_template_active_only(&name, row_id)
                        .await?;
                    info!(
                        name = %name,
                        version = %file.prompt.version,
                        "Seeded prompt from TOML"
                    );
                }
                SeedAction::SkipMatching => {}
                SeedAction::SkipOperator { created_by } => {
                    info!(
                        name = %name,
                        version = %file.prompt.version,
                        created_by = %created_by,
                        "Skipping seed: operator-edited DB row outranks TOML",
                    );
                }
                SeedAction::DriftError => {
                    return Err(OxError::Runtime {
                        message: format!(
                            "Prompt '{name}' v{} TOML diverged from seeded DB row. \
                             Bump the version in {} or update the row via \
                             POST /api/admin/prompts.",
                            file.prompt.version,
                            path.display(),
                        ),
                    });
                }
            }
        }
        Ok(())
    }

    /// Build registry from DB rows. Picks the highest version per
    /// name as defence in depth — the seed and admin paths both
    /// enforce "at most one active row per name", but an operator
    /// editing rows by hand can leave inconsistent state and the
    /// runtime should still serve a deterministic prompt.
    fn from_db_rows(rows: Vec<ox_store::PromptTemplateRow>) -> OxResult<Self> {
        let mut prompts = HashMap::new();
        let mut versions: HashMap<String, PromptVersion> = HashMap::new();

        for row in rows {
            // Skip when an already-seen name carries a higher version.
            // The DB already orders `name, version DESC`, but defending
            // here keeps the loader independent of caller ordering.
            if let Some(existing) = versions.get(&row.name)
                && *existing >= row.version
            {
                continue;
            }
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
                // `PromptTemplate.version` is the human-displayable
                // "x.y.z" string used in logs; the typed `PromptVersion`
                // lives separately in the registry's `versions` map for
                // semantic comparison.
                version: row.version.to_string(),
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

            // Already parsed during decode (`#[sqlx(try_from = "String")]`),
            // so we can insert the typed version directly.
            versions.insert(row.name.clone(), row.version);
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
                version: *ver,
                raw_version: ver.to_string(),
            })
            .collect()
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

    fn fake_row(content: &str, created_by: &str) -> ox_store::PromptTemplateRow {
        ox_store::PromptTemplateRow {
            id: uuid::Uuid::nil(),
            name: "p".to_string(),
            version: PromptVersion::parse("1.0.0").unwrap(),
            content: content.to_string(),
            variables: serde_json::json!([]),
            metadata: serde_json::json!({}),
            created_by: created_by.to_string(),
            created_at: chrono::Utc::now(),
            is_active: true,
            workspace_id: None,
        }
    }

    #[test]
    fn seed_action_inserts_when_db_row_absent() {
        assert_eq!(decide_seed_action("BODY", None), SeedAction::Insert);
    }

    #[test]
    fn seed_action_skips_matching_idempotent() {
        let row = fake_row("BODY", SYSTEM_CREATOR);
        assert_eq!(
            decide_seed_action("BODY", Some(&row)),
            SeedAction::SkipMatching,
        );
    }

    #[test]
    fn seed_action_errors_on_system_row_drift() {
        let row = fake_row("OLD BODY", SYSTEM_CREATOR);
        assert_eq!(
            decide_seed_action("NEW BODY", Some(&row)),
            SeedAction::DriftError,
        );
    }

    #[test]
    fn seed_action_yields_to_operator_edited_row() {
        let row = fake_row("OPERATOR BODY", "alice@example.com");
        assert_eq!(
            decide_seed_action("TOML BODY", Some(&row)),
            SeedAction::SkipOperator {
                created_by: "alice@example.com".to_string(),
            },
        );
    }

    fn versioned_row(name: &str, version: &str, body: &str) -> ox_store::PromptTemplateRow {
        ox_store::PromptTemplateRow {
            id: uuid::Uuid::new_v4(),
            name: name.to_string(),
            version: PromptVersion::parse(version).unwrap(),
            content: format!("[system]\n{body}\n\n[user_template]\n"),
            variables: serde_json::json!([]),
            metadata: serde_json::json!({}),
            created_by: SYSTEM_CREATOR.to_string(),
            created_at: chrono::Utc::now(),
            is_active: true,
            workspace_id: None,
        }
    }

    /// When a TOML version bump leaves the older row `is_active = true`
    /// (seed inserts a new row but never deactivates the old one — only
    /// the admin API does), the registry must still serve the highest
    /// version. Without the dedupe step, the loader's HashMap insert
    /// silently exposes whichever row arrived last regardless of order.
    #[test]
    fn from_db_rows_picks_highest_version_per_name() {
        // Mirrors postgres `ORDER BY name, version DESC` (highest first).
        let rows = vec![
            versioned_row("translate_query", "1.1.0", "NEW BODY"),
            versioned_row("translate_query", "1.0.0", "OLD BODY"),
        ];
        let registry = PromptRegistry::from_db_rows(rows).unwrap();
        let tmpl = registry.get("translate_query").unwrap();
        assert_eq!(tmpl.version, "1.1.0");
        assert_eq!(tmpl.system, "NEW BODY");
    }

    /// Same scenario in reversed order — defends against future
    /// changes to the DB query that flip the sort direction.
    #[test]
    fn from_db_rows_picks_highest_version_regardless_of_input_order() {
        let rows = vec![
            versioned_row("translate_query", "1.0.0", "OLD BODY"),
            versioned_row("translate_query", "1.1.0", "NEW BODY"),
        ];
        let registry = PromptRegistry::from_db_rows(rows).unwrap();
        let tmpl = registry.get("translate_query").unwrap();
        assert_eq!(tmpl.version, "1.1.0");
        assert_eq!(tmpl.system, "NEW BODY");
    }
}
