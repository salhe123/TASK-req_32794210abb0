use chrono::NaiveDateTime;
use diesel::prelude::*;
use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

use crate::schema::idempotency_keys;

#[derive(Debug, Clone, Queryable, Identifiable, Serialize)]
#[diesel(table_name = idempotency_keys)]
pub struct IdempotencyKey {
    pub id: Uuid,
    pub user_id: Uuid,
    pub request_id: String,
    pub method: String,
    pub path: String,
    pub status_code: i32,
    pub response_body: Value,
    pub created_at: NaiveDateTime,
    pub created_offset_minutes: i16,
    pub expires_at: NaiveDateTime,
    pub expires_offset_minutes: i16,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = idempotency_keys)]
pub struct NewIdempotencyKey {
    pub id: Uuid,
    pub user_id: Uuid,
    pub request_id: String,
    pub method: String,
    pub path: String,
    pub status_code: i32,
    pub response_body: Value,
    pub created_at: NaiveDateTime,
    pub created_offset_minutes: i16,
    pub expires_at: NaiveDateTime,
    pub expires_offset_minutes: i16,
}
