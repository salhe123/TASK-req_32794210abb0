DROP INDEX IF EXISTS idempotency_keys_user_req_method_path;
ALTER TABLE outbox_deliveries DROP COLUMN IF EXISTS next_attempt_offset_minutes;
ALTER TABLE notifications DROP COLUMN IF EXISTS read_offset_minutes;
ALTER TABLE idempotency_keys DROP COLUMN IF EXISTS expires_offset_minutes;
ALTER TABLE sessions DROP COLUMN IF EXISTS last_activity_offset_minutes;
ALTER TABLE sessions DROP COLUMN IF EXISTS expires_offset_minutes;
