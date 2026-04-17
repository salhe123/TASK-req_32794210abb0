CREATE TABLE assets (
    id UUID PRIMARY KEY,
    facility_id UUID NOT NULL REFERENCES stores(id),
    asset_label TEXT NOT NULL,
    name TEXT NOT NULL,
    status TEXT NOT NULL,
    prior_status TEXT,
    description TEXT NOT NULL DEFAULT '',
    acquired_at TIMESTAMP,
    acquired_offset_minutes SMALLINT,
    created_at TIMESTAMP NOT NULL,
    created_offset_minutes SMALLINT NOT NULL,
    updated_at TIMESTAMP NOT NULL,
    updated_offset_minutes SMALLINT NOT NULL,
    UNIQUE (facility_id, asset_label)
);
CREATE INDEX idx_assets_facility_status ON assets(facility_id, status);

CREATE TABLE asset_events (
    id UUID PRIMARY KEY,
    asset_id UUID NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
    from_status TEXT,
    to_status TEXT NOT NULL,
    actor_user_id UUID NOT NULL REFERENCES users(id),
    note TEXT NOT NULL DEFAULT '',
    created_at TIMESTAMP NOT NULL,
    created_offset_minutes SMALLINT NOT NULL
);
CREATE INDEX idx_asset_events_asset ON asset_events(asset_id);
CREATE INDEX idx_asset_events_created_at ON asset_events(created_at);

CREATE TABLE maintenance_records (
    id UUID PRIMARY KEY,
    asset_id UUID NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
    performed_at TIMESTAMP NOT NULL,
    performed_offset_minutes SMALLINT NOT NULL,
    performed_by UUID NOT NULL REFERENCES users(id),
    summary TEXT NOT NULL,
    details TEXT NOT NULL DEFAULT '',
    created_at TIMESTAMP NOT NULL,
    created_offset_minutes SMALLINT NOT NULL
);
CREATE INDEX idx_maintenance_asset ON maintenance_records(asset_id);
