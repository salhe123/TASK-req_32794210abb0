// @generated schema aligned with migrations/
diesel::table! {
    users (id) {
        id -> Uuid,
        username -> Text,
        password_hash -> Text,
        display_name -> Text,
        is_active -> Bool,
        locked_until -> Nullable<Timestamp>,
        created_at -> Timestamp,
        created_offset_minutes -> SmallInt,
        updated_at -> Timestamp,
        updated_offset_minutes -> SmallInt,
    }
}

diesel::table! {
    roles (id) {
        id -> Uuid,
        name -> Text,
        data_scope -> Text,
        field_allowlist -> Jsonb,
        created_at -> Timestamp,
        created_offset_minutes -> SmallInt,
    }
}

diesel::table! {
    permissions (id) {
        id -> Uuid,
        code -> Text,
        description -> Text,
    }
}

diesel::table! {
    role_permissions (role_id, permission_id) {
        role_id -> Uuid,
        permission_id -> Uuid,
    }
}

diesel::table! {
    user_roles (user_id, role_id) {
        user_id -> Uuid,
        role_id -> Uuid,
    }
}

diesel::table! {
    sessions (id) {
        id -> Uuid,
        user_id -> Uuid,
        token_hash -> Text,
        created_at -> Timestamp,
        created_offset_minutes -> SmallInt,
        last_activity_at -> Timestamp,
        expires_at -> Timestamp,
        revoked -> Bool,
        last_activity_offset_minutes -> SmallInt,
        expires_offset_minutes -> SmallInt,
    }
}

diesel::table! {
    login_attempts (id) {
        id -> Uuid,
        username -> Text,
        succeeded -> Bool,
        attempted_at -> Timestamp,
        attempted_offset_minutes -> SmallInt,
    }
}

diesel::table! {
    idempotency_keys (id) {
        id -> Uuid,
        user_id -> Uuid,
        request_id -> Text,
        method -> Text,
        path -> Text,
        status_code -> Integer,
        response_body -> Jsonb,
        created_at -> Timestamp,
        created_offset_minutes -> SmallInt,
        expires_at -> Timestamp,
        expires_offset_minutes -> SmallInt,
    }
}

diesel::table! {
    stores (id) {
        id -> Uuid,
        name -> Text,
        code -> Text,
        is_active -> Bool,
        created_at -> Timestamp,
        created_offset_minutes -> SmallInt,
    }
}

diesel::table! {
    audit_logs (id) {
        id -> Uuid,
        actor_user_id -> Nullable<Uuid>,
        facility_id -> Nullable<Uuid>,
        entity_type -> Text,
        entity_id -> Uuid,
        action -> Text,
        before_state -> Nullable<Jsonb>,
        after_state -> Nullable<Jsonb>,
        request_id -> Nullable<Text>,
        created_at -> Timestamp,
        created_offset_minutes -> SmallInt,
    }
}

diesel::table! {
    lost_found_items (id) {
        id -> Uuid,
        facility_id -> Uuid,
        status -> Text,
        title -> Text,
        description -> Text,
        category -> Text,
        tags -> Jsonb,
        event_date -> Nullable<Date>,
        event_time_text -> Nullable<Text>,
        location_text -> Text,
        bounce_reason -> Nullable<Text>,
        created_by -> Uuid,
        created_at -> Timestamp,
        created_offset_minutes -> SmallInt,
        updated_at -> Timestamp,
        updated_offset_minutes -> SmallInt,
    }
}

diesel::table! {
    attachment_blobs (facility_id, sha256) {
        sha256 -> Text,
        facility_id -> Uuid,
        mime_type -> Text,
        size_bytes -> BigInt,
        storage_path -> Text,
        ref_count -> Integer,
        created_at -> Timestamp,
        created_offset_minutes -> SmallInt,
    }
}

diesel::table! {
    attachments (id) {
        id -> Uuid,
        facility_id -> Uuid,
        parent_type -> Text,
        parent_id -> Uuid,
        sha256 -> Text,
        filename -> Text,
        mime_type -> Text,
        size_bytes -> BigInt,
        created_by -> Uuid,
        created_at -> Timestamp,
        created_offset_minutes -> SmallInt,
    }
}

diesel::table! {
    assets (id) {
        id -> Uuid,
        facility_id -> Uuid,
        asset_label -> Text,
        name -> Text,
        status -> Text,
        prior_status -> Nullable<Text>,
        description -> Text,
        acquired_at -> Nullable<Timestamp>,
        acquired_offset_minutes -> Nullable<SmallInt>,
        created_at -> Timestamp,
        created_offset_minutes -> SmallInt,
        updated_at -> Timestamp,
        updated_offset_minutes -> SmallInt,
    }
}

diesel::table! {
    asset_events (id) {
        id -> Uuid,
        asset_id -> Uuid,
        from_status -> Nullable<Text>,
        to_status -> Text,
        actor_user_id -> Uuid,
        note -> Text,
        created_at -> Timestamp,
        created_offset_minutes -> SmallInt,
    }
}

diesel::table! {
    maintenance_records (id) {
        id -> Uuid,
        asset_id -> Uuid,
        performed_at -> Timestamp,
        performed_offset_minutes -> SmallInt,
        performed_by -> Uuid,
        summary -> Text,
        details -> Text,
        created_at -> Timestamp,
        created_offset_minutes -> SmallInt,
    }
}

diesel::table! {
    volunteers (id) {
        id -> Uuid,
        facility_id -> Uuid,
        full_name -> Text,
        contact_email -> Nullable<Text>,
        contact_phone -> Nullable<Text>,
        gov_id_encrypted -> Nullable<Bytea>,
        gov_id_last4 -> Nullable<Text>,
        private_notes_encrypted -> Nullable<Bytea>,
        is_active -> Bool,
        created_at -> Timestamp,
        created_offset_minutes -> SmallInt,
        updated_at -> Timestamp,
        updated_offset_minutes -> SmallInt,
    }
}

diesel::table! {
    qualifications (id) {
        id -> Uuid,
        volunteer_id -> Uuid,
        kind -> Text,
        issuer -> Text,
        certificate_encrypted -> Nullable<Bytea>,
        certificate_last4 -> Nullable<Text>,
        issued_on -> Date,
        expires_on -> Nullable<Date>,
        created_at -> Timestamp,
        created_offset_minutes -> SmallInt,
    }
}

diesel::table! {
    packages (id) {
        id -> Uuid,
        facility_id -> Uuid,
        name -> Text,
        description -> Text,
        base_price -> Numeric,
        status -> Text,
        created_at -> Timestamp,
        created_offset_minutes -> SmallInt,
        updated_at -> Timestamp,
        updated_offset_minutes -> SmallInt,
        included_items -> Jsonb,
    }
}

diesel::table! {
    inventory_items (id) {
        id -> Uuid,
        facility_id -> Uuid,
        name -> Text,
        sku -> Text,
        created_at -> Timestamp,
        created_offset_minutes -> SmallInt,
    }
}

diesel::table! {
    time_slots (id) {
        id -> Uuid,
        facility_id -> Uuid,
        starts_at -> Timestamp,
        starts_offset_minutes -> SmallInt,
        ends_at -> Timestamp,
        ends_offset_minutes -> SmallInt,
        created_at -> Timestamp,
        created_offset_minutes -> SmallInt,
    }
}

diesel::table! {
    package_variants (id) {
        id -> Uuid,
        package_id -> Uuid,
        combination_key -> Text,
        price -> Numeric,
        inventory_item_id -> Nullable<Uuid>,
        time_slot_id -> Nullable<Uuid>,
        created_at -> Timestamp,
        created_offset_minutes -> SmallInt,
    }
}

diesel::table! {
    notification_templates (id) {
        id -> Uuid,
        code -> Text,
        subject -> Text,
        body -> Text,
        is_active -> Bool,
        created_at -> Timestamp,
        created_offset_minutes -> SmallInt,
        updated_at -> Timestamp,
        updated_offset_minutes -> SmallInt,
    }
}

diesel::table! {
    notifications (id) {
        id -> Uuid,
        user_id -> Uuid,
        event_kind -> Text,
        subject -> Text,
        body -> Text,
        payload -> Jsonb,
        is_read -> Bool,
        read_at -> Nullable<Timestamp>,
        created_at -> Timestamp,
        created_offset_minutes -> SmallInt,
        read_offset_minutes -> Nullable<SmallInt>,
    }
}

diesel::table! {
    outbox_deliveries (id) {
        id -> Uuid,
        user_id -> Uuid,
        event_kind -> Text,
        template_code -> Text,
        subject -> Text,
        body -> Text,
        payload -> Jsonb,
        status -> Text,
        attempt_count -> Integer,
        next_attempt_at -> Nullable<Timestamp>,
        last_error -> Nullable<Text>,
        created_at -> Timestamp,
        created_offset_minutes -> SmallInt,
        updated_at -> Timestamp,
        updated_offset_minutes -> SmallInt,
        channel -> Text,
        to_address -> Nullable<Text>,
        facility_id -> Nullable<Uuid>,
        next_attempt_offset_minutes -> Nullable<SmallInt>,
    }
}

diesel::table! {
    notification_subscriptions (user_id, event_kind) {
        user_id -> Uuid,
        event_kind -> Text,
        enabled -> Bool,
        updated_at -> Timestamp,
        updated_offset_minutes -> SmallInt,
    }
}

diesel::joinable!(role_permissions -> roles (role_id));
diesel::joinable!(role_permissions -> permissions (permission_id));
diesel::joinable!(user_roles -> users (user_id));
diesel::joinable!(user_roles -> roles (role_id));
diesel::joinable!(sessions -> users (user_id));
diesel::joinable!(idempotency_keys -> users (user_id));
diesel::joinable!(lost_found_items -> stores (facility_id));
diesel::joinable!(lost_found_items -> users (created_by));
diesel::joinable!(attachment_blobs -> stores (facility_id));
diesel::joinable!(attachments -> stores (facility_id));
diesel::joinable!(attachments -> users (created_by));
diesel::joinable!(assets -> stores (facility_id));
diesel::joinable!(asset_events -> assets (asset_id));
diesel::joinable!(asset_events -> users (actor_user_id));
diesel::joinable!(maintenance_records -> assets (asset_id));
diesel::joinable!(maintenance_records -> users (performed_by));
diesel::joinable!(volunteers -> stores (facility_id));
diesel::joinable!(qualifications -> volunteers (volunteer_id));
diesel::joinable!(packages -> stores (facility_id));
diesel::joinable!(inventory_items -> stores (facility_id));
diesel::joinable!(time_slots -> stores (facility_id));
diesel::joinable!(package_variants -> packages (package_id));
diesel::joinable!(package_variants -> inventory_items (inventory_item_id));
diesel::joinable!(package_variants -> time_slots (time_slot_id));
diesel::joinable!(notifications -> users (user_id));
diesel::joinable!(outbox_deliveries -> users (user_id));
diesel::joinable!(outbox_deliveries -> stores (facility_id));
diesel::joinable!(notification_subscriptions -> users (user_id));

diesel::allow_tables_to_appear_in_same_query!(
    users,
    roles,
    permissions,
    role_permissions,
    user_roles,
    sessions,
    login_attempts,
    idempotency_keys,
    stores,
    audit_logs,
    lost_found_items,
    attachment_blobs,
    attachments,
    assets,
    asset_events,
    maintenance_records,
    volunteers,
    qualifications,
    packages,
    inventory_items,
    time_slots,
    package_variants,
    notification_templates,
    notifications,
    outbox_deliveries,
    notification_subscriptions,
);
