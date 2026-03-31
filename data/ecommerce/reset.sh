#!/usr/bin/env bash
# ============================================================
# Reset and re-seed the E-commerce sample database
# Usage: ./data/ecommerce/reset.sh
# ============================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
DB_HOST="${ECOMMERCE_DB_HOST:-localhost}"
DB_PORT="${ECOMMERCE_DB_PORT:-5435}"
DB_NAME="${ECOMMERCE_DB_NAME:-ecommerce}"
DB_USER="${ECOMMERCE_DB_USER:-source}"
PGPASSWORD="${ECOMMERCE_DB_PASSWORD:-source-dev}"
export PGPASSWORD

echo "=== Resetting E-commerce database ==="
echo "  Host: $DB_HOST:$DB_PORT  DB: $DB_NAME  User: $DB_USER"

# Drop and recreate all tables (CASCADE handles FK dependencies)
echo "  Dropping existing tables..."
psql -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -d "$DB_NAME" -q <<'SQL'
DO $$
DECLARE
    r RECORD;
BEGIN
    FOR r IN (SELECT tablename FROM pg_tables WHERE schemaname = 'public') LOOP
        EXECUTE format('DROP TABLE IF EXISTS %I CASCADE', r.tablename);
    END LOOP;
END
$$;
SQL

# Re-create schema
echo "  Creating schema..."
psql -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -d "$DB_NAME" -q \
    -f "$SCRIPT_DIR/schema.sql"

# Load seed data
echo "  Loading seed data..."
psql -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -d "$DB_NAME" -q \
    -f "$SCRIPT_DIR/seed.sql"

# Verify counts
echo "  Verifying..."
psql -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -d "$DB_NAME" -t <<'SQL'
SELECT
    'Tables: ' || count(*)::text
FROM pg_tables WHERE schemaname = 'public';
SQL

psql -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -d "$DB_NAME" -t <<'SQL'
SELECT
    tablename || ': ' || (xpath('/row/cnt/text()', query_to_xml(format('SELECT count(*) AS cnt FROM %I', tablename), false, false, '')))[1]::text
FROM pg_tables
WHERE schemaname = 'public'
ORDER BY tablename;
SQL

echo "=== Done ==="
