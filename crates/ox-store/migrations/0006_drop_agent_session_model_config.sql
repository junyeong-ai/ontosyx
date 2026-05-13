-- Drop the unused `model_config` JSONB column from agent_sessions.
-- The column was sized for an `execution_mode` field that no longer
-- exists on the agent session surface — there is no per-session
-- model configuration carried alongside the chat run.

ALTER TABLE agent_sessions DROP COLUMN model_config;
