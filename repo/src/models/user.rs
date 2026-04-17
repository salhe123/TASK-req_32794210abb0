use chrono::NaiveDateTime;
use diesel::prelude::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::schema::users;

#[derive(Debug, Clone, Queryable, Identifiable, Serialize)]
#[diesel(table_name = users)]
pub struct User {
    pub id: Uuid,
    pub username: String,
    pub password_hash: String,
    pub display_name: String,
    pub is_active: bool,
    pub locked_until: Option<NaiveDateTime>,
    pub created_at: NaiveDateTime,
    pub created_offset_minutes: i16,
    pub updated_at: NaiveDateTime,
    pub updated_offset_minutes: i16,
}

#[derive(Debug, Insertable, Deserialize)]
#[diesel(table_name = users)]
pub struct NewUser {
    pub id: Uuid,
    pub username: String,
    pub password_hash: String,
    pub display_name: String,
    pub is_active: bool,
    pub locked_until: Option<NaiveDateTime>,
    pub created_at: NaiveDateTime,
    pub created_offset_minutes: i16,
    pub updated_at: NaiveDateTime,
    pub updated_offset_minutes: i16,
}
