-- Audit remediation: indexes, outbox channel/facility scope, package included items,
-- lost-found category enum constraint.

-- Required indexes (audit #9): standalone created_at + asset_label access paths.
CREATE INDEX IF NOT EXISTS idx_lost_found_created_at ON lost_found_items(created_at);
CREATE INDEX IF NOT EXISTS idx_assets_created_at ON assets(created_at);
CREATE INDEX IF NOT EXISTS idx_assets_asset_label ON assets(asset_label);
CREATE INDEX IF NOT EXISTS idx_packages_created_at ON packages(created_at);
CREATE INDEX IF NOT EXISTS idx_notifications_created_at ON notifications(created_at);
CREATE INDEX IF NOT EXISTS idx_volunteers_facility ON volunteers(facility_id);
CREATE INDEX IF NOT EXISTS idx_volunteers_created_at ON volunteers(created_at);

-- Outbox channel/destination model (audit #2 + #5 facility scope).
ALTER TABLE outbox_deliveries
    ADD COLUMN IF NOT EXISTS channel TEXT NOT NULL DEFAULT 'in_app',
    ADD COLUMN IF NOT EXISTS to_address TEXT,
    ADD COLUMN IF NOT EXISTS facility_id UUID REFERENCES stores(id);

ALTER TABLE outbox_deliveries
    ADD CONSTRAINT outbox_channel_allowed
    CHECK (channel IN ('in_app', 'email', 'sms', 'webhook'));

CREATE INDEX IF NOT EXISTS idx_outbox_facility ON outbox_deliveries(facility_id);
CREATE INDEX IF NOT EXISTS idx_outbox_channel ON outbox_deliveries(channel);

-- Package included items (audit #3).
ALTER TABLE packages
    ADD COLUMN IF NOT EXISTS included_items JSONB NOT NULL DEFAULT '[]'::jsonb;

-- Lost-and-found category enum constraint (audit #8).
-- The standardized occurrence/category set.
ALTER TABLE lost_found_items
    ADD CONSTRAINT lost_found_category_allowed
    CHECK (category IN ('lost', 'found', 'returned', 'damaged', 'other'));

-- New read permissions (audit #7) are seeded idempotently by services::seed::ensure_permissions.
