-- Add share_token and shared_at columns to dashboards
ALTER TABLE dashboards
    ADD COLUMN share_token VARCHAR(64) UNIQUE,
    ADD COLUMN shared_at TIMESTAMPTZ;

-- Index for public share lookup
CREATE INDEX idx_dashboards_share_token ON dashboards (share_token) WHERE share_token IS NOT NULL;
