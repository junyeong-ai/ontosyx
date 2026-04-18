-- 0010_api_key_rbac.sql: per-key role for API keys.
--
-- Previously every API key was attributed as `role = "admin"` by the
-- auth middleware, which meant a compromised non-admin automation key
-- granted platform-wide access. This migration adds a `role` column
-- the middleware consults when building the synthetic JWT claim.
--
-- Default is `'viewer'` — the safest posture for pre-existing keys.
-- Operators who need admin/designer behavior must upgrade deliberately
-- via the admin API or a manual UPDATE. The bootstrap key seeded from
-- OX_AUTH__BOOTSTRAP_KEY is created with role = 'admin' explicitly so
-- first-boot workflows keep working.

ALTER TABLE api_keys
    ADD COLUMN IF NOT EXISTS role TEXT NOT NULL DEFAULT 'viewer';

-- Enforce the same three-role vocabulary the rest of the platform uses
-- (see `Principal::Role` in ox-api). Writing another value would quietly
-- land in JWT claims and produce undefined behaviour in role checks.
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_constraint
        WHERE conname = 'api_keys_role_check'
    ) THEN
        ALTER TABLE api_keys
            ADD CONSTRAINT api_keys_role_check
            CHECK (role IN ('admin', 'designer', 'viewer'));
    END IF;
END$$;
