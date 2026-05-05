//! `RepoAnalyzer` impl for [`DefaultBrain`].

use std::collections::HashMap;

use async_trait::async_trait;

use ox_core::error::OxResult;
use ox_ontology::repo_insights::{FileContent, RepoInsights};

use crate::*;

#[async_trait]
impl RepoAnalyzer for DefaultBrain {
    async fn navigate_repo(&self, file_tree: &str) -> OxResult<Vec<String>> {
        let mut vars = HashMap::new();
        vars.insert("file_tree", file_tree);

        let selection: ox_ontology::repo_insights::FileSelection = self
            .call_structured(
                "repo_navigate",
                None,
                "repo_navigate",
                &vars,
                "Navigating repo file tree",
            )
            .await?;

        Ok(selection.files)
    }

    async fn analyze_repo_files(&self, files: &[FileContent]) -> OxResult<RepoInsights> {
        // Serialize files as a structured block for the LLM
        let files_text = files
            .iter()
            .map(|f| format!("=== {} ===\n{}", f.relative_path, f.content))
            .collect::<Vec<_>>()
            .join("\n\n");

        let mut vars = HashMap::new();
        vars.insert("files", files_text.as_str());

        self.call_structured(
            "repo_analyze",
            None,
            "repo_analyze",
            &vars,
            "Analyzing repo files for domain insights",
        )
        .await
    }
}
