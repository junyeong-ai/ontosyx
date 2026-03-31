use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::error::{OxError, OxResult};

// ---------------------------------------------------------------------------
// RepoSource — abstraction for repo access
// ---------------------------------------------------------------------------

/// Abstraction for repo access. Local path for development,
/// git URL for multi-pod production.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RepoSource {
    /// Local filesystem path (development / single-node deployment)
    Local { path: String },
    /// Remote Git repository URL (cloned on demand)
    GitUrl {
        url: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        branch: Option<String>,
    },
}

/// Result of validating a `RepoSource`. Carries the information needed to
/// actually access the repo without re-validating.
#[derive(Debug, Clone)]
pub enum ValidatedRepoSource {
    /// Canonical local filesystem path.
    Local(String),
    /// Validated remote Git URL with optional branch.
    GitUrl { url: String, branch: Option<String> },
}

impl RepoSource {
    /// Validate this source for safe access.
    ///
    /// - **Local**: canonicalizes the path and checks it lives under one of the
    ///   `allowed_roots`. Returns an error when `allowed_roots` is empty or the
    ///   path escapes.
    /// - **GitUrl**: checks the URL scheme (`https://` or `git://`), rejects
    ///   `file://` for security, and verifies the hostname is in
    ///   `allowed_git_hosts`.
    pub fn validate(
        &self,
        allowed_roots: &[String],
        allowed_git_hosts: &[String],
    ) -> OxResult<ValidatedRepoSource> {
        match self {
            RepoSource::Local { path } => Self::validate_local(path, allowed_roots),
            RepoSource::GitUrl { url, branch } => {
                Self::validate_git_url(url, branch.as_deref(), allowed_git_hosts)
            }
        }
    }

    fn validate_local(raw_path: &str, allowed_roots: &[String]) -> OxResult<ValidatedRepoSource> {
        if allowed_roots.is_empty() {
            return Err(OxError::Validation {
                field: "repo_source".into(),
                message: "Local repo enrichment is disabled: no allowed_repo_roots configured"
                    .into(),
            });
        }

        let canonical =
            std::path::Path::new(raw_path)
                .canonicalize()
                .map_err(|e| OxError::Validation {
                    field: "repo_source".into(),
                    message: format!("Cannot resolve repo path '{}': {}", raw_path, e),
                })?;

        let canonical_str = canonical.to_string_lossy();

        for root in allowed_roots {
            let root_canonical = match std::path::Path::new(root).canonicalize() {
                Ok(r) => r,
                Err(_) => continue, // Skip misconfigured roots
            };
            if canonical.starts_with(&root_canonical) {
                return Ok(ValidatedRepoSource::Local(canonical_str.into_owned()));
            }
        }

        Err(OxError::Validation {
            field: "repo_source".into(),
            message: format!("Repo path '{}' is outside all allowed roots", raw_path),
        })
    }

    fn validate_git_url(
        url: &str,
        branch: Option<&str>,
        allowed_git_hosts: &[String],
    ) -> OxResult<ValidatedRepoSource> {
        // Reject file:// protocol for security
        if url.starts_with("file://") {
            return Err(OxError::Validation {
                field: "repo_source".into(),
                message: "file:// protocol is not allowed for security reasons".into(),
            });
        }

        // Determine if this is an SCP-style SSH URL (git@host:path)
        let is_scp_style = url.starts_with("git@") && !url.contains("://");

        // Must start with https://, git://, ssh://, or be SCP-style git@host:path
        if !is_scp_style
            && !url.starts_with("https://")
            && !url.starts_with("git://")
            && !url.starts_with("ssh://")
        {
            return Err(OxError::Validation {
                field: "repo_source".into(),
                message: format!(
                    "Git URL must start with https://, git://, ssh://, or git@host:path, got: {}",
                    url
                ),
            });
        }

        if allowed_git_hosts.is_empty() {
            return Err(OxError::Validation {
                field: "repo_source".into(),
                message: "Git URL repo enrichment is disabled: no allowed_git_hosts configured"
                    .into(),
            });
        }

        // Extract hostname from URL
        let host = if is_scp_style {
            // SCP-style: git@github.com:org/repo.git → extract between '@' and ':'
            url.strip_prefix("git@")
                .and_then(|rest| rest.split(':').next())
                .ok_or_else(|| OxError::Validation {
                    field: "repo_source".into(),
                    message: format!("Cannot parse hostname from SCP-style URL: {url}"),
                })?
        } else {
            let after_scheme = url
                .split("://")
                .nth(1)
                .and_then(|rest| rest.split('/').next())
                .ok_or_else(|| OxError::Validation {
                    field: "repo_source".into(),
                    message: format!("Cannot parse hostname from URL: {url}"),
                })?;
            // Strip optional user@ prefix (e.g. git@github.com)
            let h = after_scheme.rsplit('@').next().unwrap_or(after_scheme);
            // Strip optional :port suffix
            h.split(':').next().unwrap_or(h)
        };

        if !allowed_git_hosts.iter().any(|h| h == host) {
            return Err(OxError::Validation {
                field: "repo_source".into(),
                message: format!(
                    "Git host '{}' is not in the allowed list: {:?}",
                    host, allowed_git_hosts
                ),
            });
        }

        Ok(ValidatedRepoSource::GitUrl {
            url: url.to_string(),
            branch: branch.map(|b| b.to_string()),
        })
    }
}

// ---------------------------------------------------------------------------
// FileContent — input to repo analysis LLM agent
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct FileContent {
    pub relative_path: String,
    pub content: String,
}

// ---------------------------------------------------------------------------
// FileSelection — LLM output from navigate_repo (Phase 1)
// ---------------------------------------------------------------------------

/// Structured output from the repo navigation LLM agent.
/// Contains the list of file paths selected for deeper analysis.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FileSelection {
    /// Relative paths of files to analyze (up to 30)
    pub files: Vec<String>,
}

// ---------------------------------------------------------------------------
// RepoInsights — LLM output from analyze_repo_files (Phase 2)
// ---------------------------------------------------------------------------

/// Structured insights extracted from repository source files by the LLM agent.
/// Only contains information explicitly present in the analyzed files.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RepoInsights {
    /// Detected framework/ORM (e.g., "Django", "Rails", "Spring JPA", "Prisma")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub framework: Option<String>,
    /// Enum/choice field definitions found in model/entity code
    pub enum_definitions: Vec<RepoEnumDef>,
    /// ORM-declared relationships between models
    pub orm_relationships: Vec<OrmRelationship>,
    /// Free-form hints about specific fields that help with ontology design
    pub field_hints: Vec<FieldHint>,
    /// General domain notes (e.g., "multi-tenant SaaS", "soft-delete pattern")
    pub domain_notes: Vec<String>,
    /// Files that were actually analyzed to produce these insights
    pub analyzed_files: Vec<String>,
}

// ---------------------------------------------------------------------------
// RepoEnumDef — enum/choice field definition from code
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RepoEnumDef {
    /// Model/entity name (e.g., "Store", "Order")
    pub model: String,
    /// Field name (e.g., "store_type", "status")
    pub field: String,
    /// DB table name for this model.
    /// Required for matching against the source schema. Provide the actual table name
    /// as it appears in the database (e.g., "stores", "order_items").
    /// For annotation-based ORMs, use the annotated name (@@map, Meta.db_table, @Table).
    /// For convention-based ORMs, apply the framework's naming rule (e.g., pluralize).
    pub table_name: String,
    /// All known code-label pairs
    pub values: Vec<CodeLabel>,
    /// 0.0–1.0 confidence in this extraction
    pub confidence: f32,
    /// Source file path this was extracted from
    pub source_file: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CodeLabel {
    /// The stored value (e.g., "N", "1", "ACTIVE")
    pub code: String,
    /// Human-readable label (e.g., "24시간 특화 매장", "Active")
    pub label: String,
}

// ---------------------------------------------------------------------------
// OrmRelationship — declared relationship from ORM/migration code
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct OrmRelationship {
    /// Source model (e.g., "Order")
    pub from_model: String,
    /// Target model (e.g., "Customer")
    pub to_model: String,
    /// DB table name for from_model (as it appears in the database)
    pub from_table: String,
    /// DB table name for to_model (as it appears in the database)
    pub to_table: String,
    pub relation_type: OrmRelationType,
    /// Join/through model for many-to-many (e.g., "OrderItem")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub through: Option<String>,
    /// 0.0–1.0 confidence
    pub confidence: f32,
    pub source_file: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OrmRelationType {
    BelongsTo,
    HasMany,
    HasOne,
    ManyToMany,
    HasManyThrough,
}

// ---------------------------------------------------------------------------
// FieldHint — free-form field annotation
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FieldHint {
    pub model: String,
    pub field: String,
    /// Description or domain hint (e.g., "ISO 4217 currency code", "Unix timestamp in ms")
    pub hint: String,
    pub source: String,
}
