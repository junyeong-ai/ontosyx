//! `LlmMetadata` impl for [`DefaultBrain`].

use async_trait::async_trait;

use ox_core::error::OxResult;

use crate::*;

#[async_trait]
impl LlmMetadata for DefaultBrain {
    fn default_model_info(&self) -> ProviderInfo {
        self.default_model.clone()
    }

    fn list_prompts(&self) -> Vec<(String, String)> {
        self.prompts
            .list()
            .into_iter()
            .map(|(n, v)| (n.to_string(), v.to_string()))
            .collect()
    }

    fn prompt_template_hash(&self, prompt_name: &str) -> OxResult<String> {
        // Hash the template's system body only — the user-side
        // template renders per-call with caller-supplied variables
        // (sample data, context strings, …) and a cache that wants
        // to invalidate on prompt edits but NOT on per-call shape
        // must keep those out of the fingerprint. System-only
        // hashing folds in the prompt's semantic instructions while
        // staying call-shape-agnostic.
        let tmpl = self.prompts.get(prompt_name)?;
        Ok(
            ox_ontology::source_mapping::ArtifactProvenance::compute_prompt_render_hash(
                &tmpl.system,
            ),
        )
    }
}
