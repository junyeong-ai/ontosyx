//! Knowledge utility functions.

use sha2::{Digest, Sha256};

/// Compute content hash for deduplication: SHA-256(ontology_name + lower(trim(content))).
pub fn content_hash(ontology_name: &str, content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(ontology_name.as_bytes());
    hasher.update(content.trim().to_lowercase().as_bytes());
    format!("{:x}", hasher.finalize())
}
