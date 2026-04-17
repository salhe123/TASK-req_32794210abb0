use chrono::NaiveDateTime;
use diesel::prelude::*;
use serde::Serialize;
use uuid::Uuid;

use crate::schema::{login_attempts, sessions};

#[derive(Debug, Clone, Queryable, Identifiable, Serialize)]
#[diesel(table_name = sessions)]
pub struct Session {
    pub id: Uuid,
    pub user_id: Uuid,
    pub token_hash: String,
    pub created_at: NaiveDateTime,
    pub created_offset_minutes: i16,
    pub last_activity_at: NaiveDateTime,
    pub expires_at: NaiveDateTime,
    pub revoked: bool,
    pub last_activity_offset_minutes: i16,
    pub expires_offset_minutes: i16,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = sessions)]
pub struct NewSession {
    pub id: Uuid,
    pub user_id: Uuid,
    pub token_hash: String,
    pub created_at: NaiveDateTime,
    pub created_offset_minutes: i16,
    pub last_activity_at: NaiveDateTime,
    pub expires_at: NaiveDateTime,
    pub revoked: bool,
    pub last_activity_offset_minutes: i16,
    pub expires_offset_minutes: i16,
}

#[derive(Debug, Clone, Queryable, Identifiable)]
#[diesel(table_name = login_attempts)]
pub struct LoginAttempt {
    pub id: Uuid,
    pub username: String,
    pub succeeded: bool,
    pub attempted_at: NaiveDateTime,
    pub attempted_offset_minutes: i16,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = login_attempts)]
pub struct NewLoginAttempt {
    pub id: Uuid,
    pub username: String,
    pub succeeded: bool,
    pub attempted_at: NaiveDateTime,
    pub attempted_offset_minutes: i16,
}
