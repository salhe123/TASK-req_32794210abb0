CREATE TABLE packages (
    id UUID PRIMARY KEY,
    facility_id UUID NOT NULL REFERENCES stores(id),
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    base_price NUMERIC(10,2) NOT NULL CHECK (base_price >= 0),
    status TEXT NOT NULL DEFAULT 'DRAFT',
    created_at TIMESTAMP NOT NULL,
    created_offset_minutes SMALLINT NOT NULL,
    updated_at TIMESTAMP NOT NULL,
    updated_offset_minutes SMALLINT NOT NULL
);
CREATE INDEX idx_packages_facility_status ON packages(facility_id, status);

CREATE TABLE inventory_items (
    id UUID PRIMARY KEY,
    facility_id UUID NOT NULL REFERENCES stores(id),
    name TEXT NOT NULL,
    sku TEXT NOT NULL,
    created_at TIMESTAMP NOT NULL,
    created_offset_minutes SMALLINT NOT NULL,
    UNIQUE (facility_id, sku)
);

CREATE TABLE time_slots (
    id UUID PRIMARY KEY,
    facility_id UUID NOT NULL REFERENCES stores(id),
    starts_at TIMESTAMP NOT NULL,
    starts_offset_minutes SMALLINT NOT NULL,
    ends_at TIMESTAMP NOT NULL,
    ends_offset_minutes SMALLINT NOT NULL,
    created_at TIMESTAMP NOT NULL,
    created_offset_minutes SMALLINT NOT NULL
);
CREATE INDEX idx_time_slots_facility ON time_slots(facility_id);

CREATE TABLE package_variants (
    id UUID PRIMARY KEY,
    package_id UUID NOT NULL REFERENCES packages(id) ON DELETE CASCADE,
    combination_key TEXT NOT NULL,
    price NUMERIC(10,2) NOT NULL CHECK (price >= 0),
    inventory_item_id UUID REFERENCES inventory_items(id),
    time_slot_id UUID REFERENCES time_slots(id),
    created_at TIMESTAMP NOT NULL,
    created_offset_minutes SMALLINT NOT NULL,
    UNIQUE (package_id, combination_key)
);
CREATE INDEX idx_package_variants_package ON package_variants(package_id);
