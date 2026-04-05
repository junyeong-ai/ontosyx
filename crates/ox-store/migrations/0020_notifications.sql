-- Notification channels (Slack webhook, generic webhook, etc.)
CREATE TABLE notification_channels (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id UUID NOT NULL,
    name VARCHAR(255) NOT NULL,
    channel_type VARCHAR(50) NOT NULL,  -- 'slack_webhook', 'generic_webhook'
    config JSONB NOT NULL DEFAULT '{}',  -- { "url": "...", "headers": {...} }
    events TEXT[] NOT NULL DEFAULT '{}', -- which events trigger: 'quality_rule_failed', 'quality_rule_passed', etc.
    enabled BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Notification delivery log
CREATE TABLE notification_log (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id UUID NOT NULL,
    channel_id UUID NOT NULL REFERENCES notification_channels(id) ON DELETE CASCADE,
    event_type VARCHAR(100) NOT NULL,
    subject VARCHAR(500) NOT NULL,
    body TEXT NOT NULL,
    status VARCHAR(20) NOT NULL DEFAULT 'pending',  -- 'pending', 'sent', 'failed'
    error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Indexes
CREATE INDEX idx_notification_channels_workspace ON notification_channels(workspace_id);
CREATE INDEX idx_notification_log_channel ON notification_log(channel_id);
CREATE INDEX idx_notification_log_workspace ON notification_log(workspace_id, created_at DESC);

-- RLS (FORCE ensures even table owner respects policies)
ALTER TABLE notification_channels ENABLE ROW LEVEL SECURITY;
ALTER TABLE notification_channels FORCE ROW LEVEL SECURITY;
ALTER TABLE notification_log ENABLE ROW LEVEL SECURITY;
ALTER TABLE notification_log FORCE ROW LEVEL SECURITY;

CREATE POLICY ws_isolation ON notification_channels
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON notification_channels
    USING (current_setting('app.system_bypass', true) = 'true');

CREATE POLICY ws_isolation ON notification_log
    USING (workspace_id = current_setting('app.workspace_id', true)::uuid)
    WITH CHECK (workspace_id = current_setting('app.workspace_id', true)::uuid);
CREATE POLICY system_bypass ON notification_log
    USING (current_setting('app.system_bypass', true) = 'true');
