//! `RepoAnalyzer` impl for [`DefaultBrain`].

use std::collections::HashMap;

use async_trait::async_trait;
use entelix::ExecutionContext;

use ox_core::error::OxResult;
use ox_ontology::repo_insights::{FileContent, RepoInsights};

use crate::model_resolver::operation;
use crate::*;

#[async_trait]
impl RepoAnalyzer for DefaultBrain {
    async fn navigate_repo(
        &self,
        file_tree: &str,
        ctx: &ExecutionContext,
    ) -> OxResult<Vec<String>> {
        let mut vars = HashMap::new();
        vars.insert("file_tree", file_tree);

        let selection: ox_ontology::repo_insights::FileSelection = self
            .call_structured(
                operation::REPO_NAVIGATE,
                None,
                operation::REPO_NAVIGATE,
                &vars,
                "Navigating repo file tree",
                ctx,
            )
            .await?;

        Ok(selection.files)
    }

    async fn analyze_repo_files(
        &self,
        files: &[FileContent],
        ctx: &ExecutionContext,
    ) -> OxResult<RepoInsights> {
        // Serialize files as a structured block for the LLM
        let files_text = files
            .iter()
            .map(|f| format!("=== {} ===\n{}", f.relative_path, f.content))
            .collect::<Vec<_>>()
            .join("\n\n");

        let mut vars = HashMap::new();
        vars.insert("files", files_text.as_str());

        self.call_structured(
            operation::REPO_ANALYZE,
            None,
            operation::REPO_ANALYZE,
            &vars,
            "Analyzing repo files for domain insights",
            ctx,
        )
        .await
    }
}
