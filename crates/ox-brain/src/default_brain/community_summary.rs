//! `CommunitySummariser` impl for [`DefaultBrain`].

use std::collections::HashMap;

use async_trait::async_trait;

use ox_core::error::OxResult;

use crate::model_resolver::operation;
use crate::*;

#[async_trait]
impl CommunitySummariser for DefaultBrain {
    async fn summarise_community(
        &self,
        request: CommunitySummaryRequest<'_>,
    ) -> OxResult<(CommunitySummaryResponse, CallProvenance)> {
        // Render the member list as a single string the prompt
        // template substitutes verbatim. One member per line:
        // `- {kind}: {display_name} (id={logical_id})` keeps the
        // information ordered the way an operator would read
        // it, with the display name (the LLM's main signal) up
        // front. The cron pre-sorts members deterministically,
        // so the rendered text is reproducible across runs and
        // the call's `prompt_render_hash` is stable.
        let mut members_block = String::with_capacity(request.members.len() * 64);
        for m in request.members {
            let display = if m.display_name.is_empty() {
                m.logical_id
            } else {
                m.display_name
            };
            members_block.push_str("- ");
            members_block.push_str(m.kind);
            members_block.push_str(": ");
            members_block.push_str(display);
            members_block.push_str(" (id=");
            members_block.push_str(m.logical_id);
            members_block.push_str(")\n");
        }
        let member_count_str = request.members.len().to_string();

        let mut vars: HashMap<&str, &str> = HashMap::new();
        vars.insert("workspace_name", request.workspace_name);
        vars.insert("members", members_block.as_str());
        vars.insert("member_count", member_count_str.as_str());

        self.call_structured_traced(
            operation::COMMUNITY_SUMMARY,
            None,
            operation::COMMUNITY_SUMMARY,
            &vars,
            "Summarising community for GraphRAG retrieval",
        )
        .await
    }
}
