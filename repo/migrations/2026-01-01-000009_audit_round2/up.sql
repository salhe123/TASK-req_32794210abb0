-- Audit round 2 remediation:
--  * Uniform timestamp + offset across all persisted timestamp fields.
--  * Method/path binding for idempotency replay.
--  * Per-channel outbox attempt timestamp offset.

-- Sessions: last_activity_at and expires_at must carry offsets.
ALTER TABLE sessions
    ADD COLUMN IF NOT EXISTS last_activity_offset_minutes SMALLINT NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS expires_offset_minutes SMALLINT NOT NULL DEFAULT 0;

-- Idempotency keys: expiry time needs its offset companion too.
ALTER TABLE idempotency_keys
    ADD COLUMN IF NOT EXISTS expires_offset_minutes SMALLINT NOT NULL DEFAULT 0;

-- Notifications: read_at must pair with read_offset_minutes (NULL when unread).
ALTER TABLE notifications
    ADD COLUMN IF NOT EXISTS read_offset_minutes SMALLINT;

-- Outbox deliveries: next_attempt_at must pair with an offset.
ALTER TABLE outbox_deliveries
    ADD COLUMN IF NOT EXISTS next_attempt_offset_minutes SMALLINT;

-- Idempotency: replay must key on (user_id, request_id, method, path). The
-- prior unique key was just request_id globally; widen so that using the same
-- request_id across different endpoints is treated as a brand-new request.
-- We keep the existing primary key (id) and add a targeted uniqueness index
-- used by the ON CONFLICT clause.
CREATE UNIQUE INDEX IF NOT EXISTS idempotency_keys_user_req_method_path
    ON idempotency_keys(user_id, request_id, method, path);
