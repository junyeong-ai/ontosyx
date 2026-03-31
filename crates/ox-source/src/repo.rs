use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Duration;

use ignore::WalkBuilder;
use ox_core::error::{OxError, OxResult};
use ox_core::repo_insights::{FileContent, OrmRelationType, RepoInsights};
use ox_core::source_schema::{
    ForeignKeyDef, SourceColumnDef, SourceProfile, SourceSchema, SourceTableDef, TableProfile,
};
use tempfile::TempDir;

const MAX_FILE_SIZE_BYTES: u64 = 100_000; // 100 KB per file
const MAX_TOTAL_CONTENT_BYTES: usize = 150_000; // 150 KB total across all files
const MAX_TREE_ENTRIES: usize = 3_000; // file tree lines sent to LLM navigator

const EXCLUDED_EXTENSIONS: &[&str] = &[
    "class", "jar", "pyc", "pyo", "pyd", "so", "dylib", "dll", "exe", "bin", "o", "a", "lib",
    "wasm", "jpg", "jpeg", "png", "gif", "svg", "ico", "webp", "bmp", "tiff", "pdf", "zip", "tar",
    "gz", "bz2", "xz", "7z", "rar", "lock", "sum",
];

// ---------------------------------------------------------------------------
// RepoIntrospector
// ---------------------------------------------------------------------------

pub struct RepoIntrospector {
    root: PathBuf,
}

impl RepoIntrospector {
    /// Create a new introspector rooted at `path`.
    /// Validates that the path exists and is a directory.
    pub fn new(path: &str) -> OxResult<Self> {
        let root = PathBuf::from(path)
            .canonicalize()
            .map_err(|e| OxError::Runtime {
                message: format!("Cannot access repo path '{path}': {e}"),
            })?;

        if !root.is_dir() {
            return Err(OxError::Runtime {
                message: format!("Repo path '{path}' is not a directory"),
            });
        }

        Ok(Self { root })
    }

    /// Generate a compact file tree string for the repo.
    /// Respects .gitignore, .ignore, and global gitignore rules via the `ignore` crate.
    /// Returns (tree_string, was_truncated).
    pub fn generate_tree(&self) -> OxResult<(String, bool)> {
        let mut lines = Vec::new();
        lines.push(format!("{}/", self.root.display()));

        let walker = WalkBuilder::new(&self.root)
            .git_ignore(true)
            .git_global(true)
            .git_exclude(true)
            .hidden(true) // skip hidden files unless gitignore says otherwise
            .sort_by_file_name(|a, b| a.cmp(b))
            .build();

        for result in walker {
            let entry = result.map_err(|e| OxError::Runtime {
                message: format!("Error walking repo: {e}"),
            })?;

            // Skip the root itself
            if entry.path() == self.root {
                continue;
            }

            let path = entry.path();
            let relative = path
                .strip_prefix(&self.root)
                .map_err(|_| OxError::Runtime {
                    message: "Path strip_prefix failed".to_string(),
                })?;

            // Skip excluded file types from the tree listing
            if let Some(ext) = relative.extension().and_then(|e| e.to_str())
                && EXCLUDED_EXTENSIONS.contains(&ext.to_lowercase().as_str())
            {
                continue;
            }

            let depth = relative.components().count();
            let indent = "  ".repeat(depth.saturating_sub(1));
            let name = relative.file_name().and_then(|n| n.to_str()).unwrap_or("");

            if path.is_dir() {
                lines.push(format!("{indent}{name}/"));
            } else {
                lines.push(format!("{indent}{name}"));
            }
        }

        let was_truncated = lines.len() > MAX_TREE_ENTRIES;
        if was_truncated {
            let truncated = lines.len() - MAX_TREE_ENTRIES;
            lines.truncate(MAX_TREE_ENTRIES);
            lines.push(format!(
                "... ({truncated} more entries truncated — provide repo_path to a focused subdirectory)"
            ));
        }

        Ok((lines.join("\n"), was_truncated))
    }

    /// Read the contents of selected files by relative path.
    /// Enforces per-file and total size limits to prevent LLM context overflow.
    pub fn read_files(&self, paths: &[String]) -> OxResult<Vec<FileContent>> {
        let mut results = Vec::new();
        let mut total_bytes = 0usize;

        for relative_path in paths {
            if total_bytes >= MAX_TOTAL_CONTENT_BYTES {
                tracing::warn!(
                    "Repo file reading stopped at total limit ({MAX_TOTAL_CONTENT_BYTES} bytes). \
                     {} files remaining.",
                    paths.len() - results.len()
                );
                break;
            }

            let abs_path = match self.validate_path(relative_path) {
                Ok(p) => p,
                Err(e) => {
                    tracing::warn!("Skipping '{}': {e}", relative_path);
                    continue;
                }
            };

            // Skip excluded file types
            if let Some(ext) = abs_path.extension().and_then(|e| e.to_str())
                && EXCLUDED_EXTENSIONS.contains(&ext.to_lowercase().as_str())
            {
                tracing::debug!("Skipping excluded file: {relative_path}");
                continue;
            }

            // Enforce per-file size limit
            let metadata = match std::fs::metadata(&abs_path) {
                Ok(m) => m,
                Err(e) => {
                    tracing::warn!("Cannot stat '{}': {e}", relative_path);
                    continue;
                }
            };

            if metadata.len() > MAX_FILE_SIZE_BYTES {
                tracing::warn!(
                    "Skipping '{}': {} bytes exceeds limit of {MAX_FILE_SIZE_BYTES}",
                    relative_path,
                    metadata.len()
                );
                continue;
            }

            let content = match std::fs::read_to_string(&abs_path) {
                Ok(c) => c,
                Err(e) => {
                    tracing::warn!("Cannot read '{}': {e}", relative_path);
                    continue;
                }
            };

            total_bytes += content.len();
            results.push(FileContent {
                relative_path: relative_path.clone(),
                content,
            });
        }

        Ok(results)
    }

    /// Validate that a relative path is safe (no path traversal) and exists.
    fn validate_path(&self, relative_path: &str) -> OxResult<PathBuf> {
        // Reject obvious traversal attempts early
        if relative_path.contains("..") {
            return Err(OxError::Runtime {
                message: format!("Path traversal rejected: '{relative_path}'"),
            });
        }

        let abs = self.root.join(relative_path);

        // Canonicalize and verify it's still under root
        let canonical = abs.canonicalize().map_err(|e| OxError::Runtime {
            message: format!("Cannot resolve path '{relative_path}': {e}"),
        })?;

        if !canonical.starts_with(&self.root) {
            return Err(OxError::Runtime {
                message: format!("Path escapes repo root: '{relative_path}'"),
            });
        }

        if !canonical.is_file() {
            return Err(OxError::Runtime {
                message: format!("Not a file: '{relative_path}'"),
            });
        }

        Ok(canonical)
    }
}

// ---------------------------------------------------------------------------
// Git clone support
// ---------------------------------------------------------------------------

const GIT_CLONE_TIMEOUT: Duration = Duration::from_secs(60);

/// Result of a git clone operation.
pub struct CloneResult {
    pub introspector: RepoIntrospector,
    pub tmpdir: TempDir,
    /// HEAD commit SHA of the cloned repository (for reproducibility).
    pub commit_sha: String,
}

/// Clone a remote Git repository into a temporary directory and return
/// an introspector rooted at the clone, along with the HEAD commit SHA.
///
/// The caller **must** hold the returned `TempDir` for the duration of use;
/// dropping it deletes the cloned files.
pub async fn clone_repo(url: &str, branch: Option<&str>) -> OxResult<CloneResult> {
    let tmpdir = TempDir::new().map_err(|e| OxError::Runtime {
        message: format!("Failed to create temp directory for git clone: {e}"),
    })?;

    let dest = tmpdir.path().to_string_lossy().to_string();

    let mut cmd = tokio::process::Command::new("git");
    cmd.arg("clone")
        .arg("--depth")
        .arg("1")
        .arg("--single-branch");

    if let Some(b) = branch {
        cmd.arg("--branch").arg(b);
    }

    cmd.arg(url).arg(&dest);

    tracing::info!(url, branch, dest = %dest, "Cloning git repository");

    let output = tokio::time::timeout(GIT_CLONE_TIMEOUT, cmd.output())
        .await
        .map_err(|_| OxError::Runtime {
            message: format!(
                "Git clone timed out after {}s for '{url}'",
                GIT_CLONE_TIMEOUT.as_secs()
            ),
        })?
        .map_err(|e| OxError::Runtime {
            message: format!("Failed to execute git clone: {e}"),
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(OxError::Runtime {
            message: format!("Git clone failed for '{url}': {stderr}"),
        });
    }

    // Capture HEAD commit SHA for reproducibility
    let sha_output = tokio::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(&dest)
        .output()
        .await
        .map_err(|e| OxError::Runtime {
            message: format!("Failed to read commit SHA: {e}"),
        })?;

    let commit_sha = String::from_utf8_lossy(&sha_output.stdout)
        .trim()
        .to_string();

    tracing::info!(url, commit_sha = %commit_sha, "Git clone completed");

    let introspector = RepoIntrospector::new(&dest)?;
    Ok(CloneResult {
        introspector,
        tmpdir,
        commit_sha,
    })
}

// ---------------------------------------------------------------------------
// RepoInsights → SourceSchema conversion
// ---------------------------------------------------------------------------

/// Convert LLM-extracted repo insights into a SourceSchema.
///
/// Maps ORM models to tables (via `table_name` fields in enum_definitions and
/// ORM relationships) and infers columns from enum field hints. Relationships
/// become ForeignKeyDefs with `inferred = true`.
///
/// This produces a best-effort schema from code analysis — it may have fewer
/// columns than a real database introspection since only fields the LLM
/// identified are included.
pub fn repo_insights_to_schema(insights: &RepoInsights) -> (SourceSchema, SourceProfile) {
    let mut table_map: std::collections::HashMap<String, Vec<SourceColumnDef>> =
        std::collections::HashMap::new();
    let mut table_pks: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();

    // Collect tables and columns from enum definitions
    for enum_def in &insights.enum_definitions {
        let table_name = enum_def.table_name.clone();
        let entry = table_map.entry(table_name.clone()).or_default();

        // Add the enum field as a column if not already present
        if !entry.iter().any(|c| c.name == enum_def.field) {
            entry.push(SourceColumnDef {
                name: enum_def.field.clone(),
                data_type: "varchar".to_string(),
                nullable: false,
            });
        }
    }

    // Collect tables from ORM relationships
    for rel in &insights.orm_relationships {
        // Ensure both tables exist
        table_map.entry(rel.from_table.clone()).or_default();
        table_map.entry(rel.to_table.clone()).or_default();
    }

    // Collect hints about fields
    for hint in &insights.field_hints {
        // Try to find the table_name for this model from enum_definitions
        let table_name = insights
            .enum_definitions
            .iter()
            .find(|e| e.model == hint.model)
            .map(|e| e.table_name.clone())
            .or_else(|| {
                // Try ORM relationships (from_model or to_model)
                insights.orm_relationships.iter().find_map(|r| {
                    if r.from_model == hint.model {
                        Some(r.from_table.clone())
                    } else if r.to_model == hint.model {
                        Some(r.to_table.clone())
                    } else {
                        None
                    }
                })
            });

        if let Some(table_name) = table_name {
            let entry = table_map.entry(table_name).or_default();
            if !entry.iter().any(|c| c.name == hint.field) {
                entry.push(SourceColumnDef {
                    name: hint.field.clone(),
                    data_type: "varchar".to_string(),
                    nullable: true,
                });
            }
        }
    }

    // Ensure every table has at least an "id" PK column
    for (table_name, columns) in &mut table_map {
        if !columns.iter().any(|c| c.name == "id") {
            columns.insert(
                0,
                SourceColumnDef {
                    name: "id".to_string(),
                    data_type: "int".to_string(),
                    nullable: false,
                },
            );
        }
        table_pks
            .entry(table_name.clone())
            .or_insert_with(|| vec!["id".to_string()]);
    }

    // Build foreign keys from ORM relationships
    let mut foreign_keys = Vec::new();
    let mut seen_fks: HashSet<(String, String, String, String)> = HashSet::new();

    for rel in &insights.orm_relationships {
        // For BelongsTo / HasMany / HasOne, infer a FK column on the "from" side
        let (from_table, from_column, to_table, to_column) = match rel.relation_type {
            OrmRelationType::BelongsTo => {
                // from_model belongs_to to_model → FK on from_table
                let fk_col = format!("{}_id", naive_singularize(&rel.to_table));
                (
                    rel.from_table.clone(),
                    fk_col,
                    rel.to_table.clone(),
                    "id".to_string(),
                )
            }
            OrmRelationType::HasMany | OrmRelationType::HasOne => {
                // from_model has_many/has_one to_model → FK on to_table
                let fk_col = format!("{}_id", naive_singularize(&rel.from_table));
                (
                    rel.to_table.clone(),
                    fk_col,
                    rel.from_table.clone(),
                    "id".to_string(),
                )
            }
            OrmRelationType::ManyToMany | OrmRelationType::HasManyThrough => {
                // Skip — these involve join tables which we may not have info about
                continue;
            }
        };

        let key = (
            from_table.clone(),
            from_column.clone(),
            to_table.clone(),
            to_column.clone(),
        );
        if seen_fks.contains(&key) {
            continue;
        }
        seen_fks.insert(key);

        // Ensure the FK column exists in the source table
        if let Some(cols) = table_map.get_mut(&from_table)
            && !cols.iter().any(|c| c.name == from_column)
        {
            cols.push(SourceColumnDef {
                name: from_column.clone(),
                data_type: "int".to_string(),
                nullable: true,
            });
        }

        foreign_keys.push(ForeignKeyDef {
            from_table,
            from_column,
            to_table,
            to_column,
            inferred: true,
        });
    }

    // Build SourceTableDefs
    let mut tables: Vec<SourceTableDef> = table_map
        .into_iter()
        .map(|(name, columns)| {
            let primary_key = table_pks
                .get(&name)
                .cloned()
                .unwrap_or_else(|| vec!["id".to_string()]);
            SourceTableDef {
                name,
                columns,
                primary_key,
            }
        })
        .collect();
    tables.sort_by(|a, b| a.name.cmp(&b.name));

    // Build a minimal SourceProfile (no actual data to profile)
    let table_profiles = tables
        .iter()
        .map(|t| TableProfile {
            table_name: t.name.clone(),
            row_count: 0,
            column_stats: Vec::new(),
        })
        .collect();

    let schema = SourceSchema {
        source_type: "code_repository".to_string(),
        tables,
        foreign_keys,
    };

    let profile = SourceProfile { table_profiles };

    (schema, profile)
}

/// Convert a plural table name to a reasonable singular form for FK column naming.
/// Handles common English pluralization patterns beyond simple trailing 's'.
fn naive_singularize(name: &str) -> &str {
    if let Some(base) = name.strip_suffix("ies") {
        // companies → company (but we return just the base for "{base}_id" → "compan_id"
        // isn't right either, so skip this pattern and use full name)
        // Actually: the convention is "{singular}_id", but we can't reconstruct "company"
        // from "companies" without a dictionary. Use the table name as-is for safety.
        let _ = base;
        return name;
    }
    if name.ends_with("ses") || name.ends_with("xes") || name.ends_with("zes") {
        // addresses → address, boxes → box, quizzes → quiz
        return &name[..name.len() - 2];
    }
    if name.ends_with("ches") || name.ends_with("shes") {
        // watches → watch, dishes → dish
        return &name[..name.len() - 2];
    }
    if let Some(base) = name.strip_suffix('s') {
        // users → user, products → product
        // But don't strip from: status, class, address (already handled above)
        if !base.ends_with('s') && !base.ends_with('u') {
            return base;
        }
    }
    // Not plural or can't determine singular: use as-is
    name
}
