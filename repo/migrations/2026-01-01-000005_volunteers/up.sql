CREATE TABLE volunteers (
    id UUID PRIMARY KEY,
    facility_id UUID NOT NULL REFERENCES stores(id),
    full_name TEXT NOT NULL,
    contact_email TEXT,
    contact_phone TEXT,
    gov_id_encrypted BYTEA,
    gov_id_last4 TEXT,
    private_notes_encrypted BYTEA,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMP NOT NULL,
    created_offset_minutes SMALLINT NOT NULL,
    updated_at TIMESTAMP NOT NULL,
    updated_offset_minutes SMALLINT NOT NULL
);
CREATE INDEX idx_volunteers_facility ON volunteers(facility_id);

CREATE TABLE qualifications (
    id UUID PRIMARY KEY,
    volunteer_id UUID NOT NULL REFERENCES volunteers(id) ON DELETE CASCADE,
    kind TEXT NOT NULL,
    issuer TEXT NOT NULL,
    certificate_encrypted BYTEA,
    certificate_last4 TEXT,
    issued_on DATE NOT NULL,
    expires_on DATE,
    created_at TIMESTAMP NOT NULL,
    created_offset_minutes SMALLINT NOT NULL
);
CREATE INDEX idx_qualifications_volunteer ON qualifications(volunteer_id);
CREATE INDEX idx_qualifications_expires_on ON qualifications(expires_on);
