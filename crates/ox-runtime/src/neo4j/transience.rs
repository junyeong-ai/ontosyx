use crate::TransienceDetector;

/// Neo4j-specific transient error detection.
///
/// Rules and regression cases live in `crate::transience`. Adding a new
/// false-positive case is a one-line rule + test there.
pub struct Neo4jTransienceDetector;

impl TransienceDetector for Neo4jTransienceDetector {
    fn is_transient(&self, err_msg: &str) -> bool {
        crate::transience::classify(&crate::transience::NEO4J_RULES, err_msg).is_transient()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_neo4j_transience_detector_various_messages() {
        let detector = Neo4jTransienceDetector;

        assert!(detector.is_transient("Connection reset by peer"));
        assert!(detector.is_transient("broken pipe"));
        assert!(detector.is_transient("Connection refused"));
        assert!(detector.is_transient("request timed out"));
        assert!(detector.is_transient("operation timeout"));
        assert!(detector.is_transient("Too many requests"));
        assert!(detector.is_transient("Service unavailable"));
        assert!(detector.is_transient("Leader switch in progress"));
        assert!(detector.is_transient("Database no longer available"));
        assert!(detector.is_transient("database unavailable"));

        assert!(detector.is_transient("CONNECTION RESET"));
        assert!(detector.is_transient("BROKEN PIPE"));

        assert!(!detector.is_transient("Syntax error in Cypher"));
        assert!(!detector.is_transient("Node not found"));
        assert!(!detector.is_transient("Permission denied"));
        assert!(!detector.is_transient("Invalid query"));
        assert!(!detector.is_transient(""));
    }
}
