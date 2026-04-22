-- Create non-superuser application role for RLS enforcement.
-- PostgreSQL superusers bypass FORCE ROW LEVEL SECURITY;
-- the app must connect as a non-superuser for workspace isolation to work.
DO $$
BEGIN
  IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = 'ontosyx_app') THEN
    CREATE ROLE ontosyx_app LOGIN PASSWORD 'ontosyx-dev' NOSUPERUSER;
  END IF;
END
$$;

-- Required Postgres extensions used by migrations 0001+:
--   vector  — pgvector (embedding columns on knowledge / memory tables)
--   pg_trgm — trigram indexes for fuzzy search
-- Must be created as the superuser (POSTGRES_USER) before the app user
-- opens its first connection, since migrations run as `ontosyx_app`.
CREATE EXTENSION IF NOT EXISTS vector;
CREATE EXTENSION IF NOT EXISTS pg_trgm;

-- Grant full access to public schema objects
GRANT ALL ON ALL TABLES IN SCHEMA public TO ontosyx_app;
GRANT ALL ON ALL SEQUENCES IN SCHEMA public TO ontosyx_app;
GRANT USAGE, CREATE ON SCHEMA public TO ontosyx_app;
ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT ALL ON TABLES TO ontosyx_app;
ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT ALL ON SEQUENCES TO ontosyx_app;
