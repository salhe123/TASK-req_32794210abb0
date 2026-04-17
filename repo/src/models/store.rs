use chrono::NaiveDateTime;
use diesel::prelude::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::schema::stores;

#[derive(Debug, Clone, Queryable, Identifiable, Serialize)]
#[diesel(table_name = stores)]
pub struct Store {
    pub id: Uuid,
    pub name: String,
    pub code: String,
    pub is_active: bool,
    pub created_at: NaiveDateTime,
    pub created_offset_minutes: i16,
}

#[derive(Debug, Insertable, Deserialize)]
#[diesel(table_name = stores)]
pub struct NewStore {
    pub id: Uuid,
    pub name: String,
    pub code: String,
    pub is_active: bool,
    pub created_at: NaiveDateTime,
    pub created_offset_minutes: i16,
}
