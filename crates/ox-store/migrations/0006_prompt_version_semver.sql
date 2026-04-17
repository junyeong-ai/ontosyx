-- Phase A1 — enforce semver shape on prompt_templates.version
--
-- The Rust side now decodes the column via `try_from = "String"` into
-- `ox_core::PromptVersion { major, minor, patch }`. Adding a CHECK
-- constraint here fails fast on any pre-existing or out-of-band write
-- that doesn't match `<u32>.<u32>.<u32>`, so the decode side never has
-- to handle malformed rows at runtime.
--
-- Existing rows are validated (the constraint is NOT marked NOT VALID)
-- so the migration fails loud if the seed data has bad versions —
-- preferable to silently failing later from inside `prompt_registry::load_from_db`.

ALTER TABLE prompt_templates
    DROP CONSTRAINT IF EXISTS prompt_templates_version_semver_chk;

ALTER TABLE prompt_templates
    ADD CONSTRAINT prompt_templates_version_semver_chk
    CHECK (version ~ '^[0-9]+\.[0-9]+\.[0-9]+$');
