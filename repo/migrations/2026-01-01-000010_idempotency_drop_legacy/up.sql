-- Audit round 3 / Blocker #1: the original migration created
--   UNIQUE (user_id, request_id)
-- on idempotency_keys, which conflicts with the composite (user_id, request_id,
-- method, path) replay key introduced in round 2. Without dropping this legacy
-- constraint, a same-user retry of the same request_id across two different
-- endpoints trips the old unique violation and surfaces as a 500.
--
-- We look up the constraint by its auto-generated default name and drop it
-- idempotently so this migration is safe on fresh and upgraded DBs.

ALTER TABLE idempotency_keys
    DROP CONSTRAINT IF EXISTS idempotency_keys_user_id_request_id_key;

-- In case the constraint was materialised as an index under a different name,
-- also drop any non-composite unique on (user_id, request_id) that matches.
DO $$
DECLARE
    v_index_name text;
BEGIN
    SELECT indexrelid::regclass::text INTO v_index_name
    FROM pg_index i
    JOIN pg_class c ON c.oid = i.indrelid
    WHERE c.relname = 'idempotency_keys'
      AND i.indisunique
      AND NOT i.indisprimary
      AND (
          SELECT array_agg(attname ORDER BY attnum)
          FROM pg_attribute
          WHERE attrelid = i.indrelid AND attnum = ANY(i.indkey)
      ) = ARRAY['user_id'::name, 'request_id'::name]
    LIMIT 1;
    IF v_index_name IS NOT NULL THEN
        EXECUTE format('DROP INDEX IF EXISTS %s', v_index_name);
    END IF;
END$$;
