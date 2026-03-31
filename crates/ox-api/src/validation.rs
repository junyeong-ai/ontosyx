use ox_core::ontology_ir::OntologyIR;

use crate::error::AppError;

// ---------------------------------------------------------------------------
// Length validation utilities
// ---------------------------------------------------------------------------

/// Maximum length for short text fields (names, titles).
const MAX_NAME_LENGTH: usize = 200;
/// Maximum length for description/text fields.
const MAX_DESCRIPTION_LENGTH: usize = 5000;
/// Maximum length for user messages (chat, queries).
const MAX_MESSAGE_LENGTH: usize = 50_000;
/// Maximum length for code templates (recipes).
const MAX_CODE_LENGTH: usize = 100_000;

/// Validate that a string field does not exceed the given maximum length.
pub fn validate_length(field_name: &str, value: &str, max_length: usize) -> Result<(), AppError> {
    if value.len() > max_length {
        return Err(AppError::validation(
            field_name,
            &format!(
                "exceeds maximum length of {max_length} characters (got {})",
                value.len()
            ),
        ));
    }
    Ok(())
}

/// Validate a name field (short text, max 200 chars).
pub fn validate_name(field_name: &str, value: &str) -> Result<(), AppError> {
    if value.trim().is_empty() {
        return Err(AppError::validation(field_name, "cannot be empty"));
    }
    validate_length(field_name, value, MAX_NAME_LENGTH)
}

/// Validate a description field (max 5000 chars).
pub fn validate_description(field_name: &str, value: &str) -> Result<(), AppError> {
    validate_length(field_name, value, MAX_DESCRIPTION_LENGTH)
}

/// Validate a user message field (max 50K chars).
pub fn validate_message(field_name: &str, value: &str) -> Result<(), AppError> {
    if value.trim().is_empty() {
        return Err(AppError::validation(field_name, "cannot be empty"));
    }
    validate_length(field_name, value, MAX_MESSAGE_LENGTH)
}

/// Validate a code template field (max 100K chars).
pub fn validate_code(field_name: &str, value: &str) -> Result<(), AppError> {
    validate_length(field_name, value, MAX_CODE_LENGTH)
}

// ---------------------------------------------------------------------------
// Ontology input validation
// ---------------------------------------------------------------------------

pub fn validate_ontology_input(ontology: &OntologyIR) -> Result<(), AppError> {
    let errors = ontology.validate();
    if errors.is_empty() {
        return Ok(());
    }

    Err(AppError::unprocessable_with_details(
        "invalid_ontology",
        "Ontology validation failed",
        serde_json::json!({ "errors": errors }),
    ))
}
