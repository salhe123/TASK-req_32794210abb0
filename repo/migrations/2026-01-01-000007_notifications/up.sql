CREATE TABLE notification_templates (
    id UUID PRIMARY KEY,
    code TEXT NOT NULL UNIQUE,
    subject TEXT NOT NULL,
    body TEXT NOT NULL,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMP NOT NULL,
    created_offset_minutes SMALLINT NOT NULL,
    updated_at TIMESTAMP NOT NULL,
    updated_offset_minutes SMALLINT NOT NULL
);

CREATE TABLE notifications (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    event_kind TEXT NOT NULL,
    subject TEXT NOT NULL,
    body TEXT NOT NULL,
    payload JSONB NOT NULL DEFAULT '{}'::jsonb,
    is_read BOOLEAN NOT NULL DEFAULT FALSE,
    read_at TIMESTAMP,
    created_at TIMESTAMP NOT NULL,
    created_offset_minutes SMALLINT NOT NULL
);
CREATE INDEX idx_notifications_user_read ON notifications(user_id, is_read);

CREATE TABLE outbox_deliveries (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    event_kind TEXT NOT NULL,
    template_code TEXT NOT NULL,
    subject TEXT NOT NULL,
    body TEXT NOT NULL,
    payload JSONB NOT NULL DEFAULT '{}'::jsonb,
    status TEXT NOT NULL DEFAULT 'PENDING',
    attempt_count INTEGER NOT NULL DEFAULT 0,
    next_attempt_at TIMESTAMP,
    last_error TEXT,
    created_at TIMESTAMP NOT NULL,
    created_offset_minutes SMALLINT NOT NULL,
    updated_at TIMESTAMP NOT NULL,
    updated_offset_minutes SMALLINT NOT NULL
);
CREATE INDEX idx_outbox_status ON outbox_deliveries(status);
CREATE INDEX idx_outbox_created_at ON outbox_deliveries(created_at);

CREATE TABLE notification_subscriptions (
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    event_kind TEXT NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    updated_at TIMESTAMP NOT NULL,
    updated_offset_minutes SMALLINT NOT NULL,
    PRIMARY KEY (user_id, event_kind)
);
