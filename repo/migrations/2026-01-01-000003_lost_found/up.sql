CREATE TABLE lost_found_items (
    id UUID PRIMARY KEY,
    facility_id UUID NOT NULL REFERENCES stores(id),
    status TEXT NOT NULL,
    title TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    category TEXT NOT NULL,
    tags JSONB NOT NULL DEFAULT '[]'::jsonb,
    event_date DATE,
    event_time_text TEXT,
    location_text TEXT NOT NULL DEFAULT '',
    bounce_reason TEXT,
    created_by UUID NOT NULL REFERENCES users(id),
    created_at TIMESTAMP NOT NULL,
    created_offset_minutes SMALLINT NOT NULL,
    updated_at TIMESTAMP NOT NULL,
    updated_offset_minutes SMALLINT NOT NULL
);
CREATE INDEX idx_lost_found_facility_status ON lost_found_items(facility_id, status);

CREATE TABLE attachment_blobs (
    sha256 TEXT NOT NULL,
    facility_id UUID NOT NULL REFERENCES stores(id),
    mime_type TEXT NOT NULL,
    size_bytes BIGINT NOT NULL,
    storage_path TEXT NOT NULL,
    ref_count INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMP NOT NULL,
    created_offset_minutes SMALLINT NOT NULL,
    PRIMARY KEY (facility_id, sha256)
);

CREATE TABLE attachments (
    id UUID PRIMARY KEY,
    facility_id UUID NOT NULL REFERENCES stores(id),
    parent_type TEXT NOT NULL,
    parent_id UUID NOT NULL,
    sha256 TEXT NOT NULL,
    filename TEXT NOT NULL,
    mime_type TEXT NOT NULL,
    size_bytes BIGINT NOT NULL,
    created_by UUID NOT NULL REFERENCES users(id),
    created_at TIMESTAMP NOT NULL,
    created_offset_minutes SMALLINT NOT NULL,
    FOREIGN KEY (facility_id, sha256) REFERENCES attachment_blobs(facility_id, sha256)
);
CREATE INDEX idx_attachments_facility_sha ON attachments(facility_id, sha256);
CREATE INDEX idx_attachments_parent ON attachments(parent_type, parent_id);
