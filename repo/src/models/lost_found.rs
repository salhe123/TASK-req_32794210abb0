use chrono::{NaiveDate, NaiveDateTime};
use diesel::prelude::*;
use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

use crate::schema::lost_found_items;

#[derive(Debug, Clone, Queryable, Identifiable, Serialize, AsChangeset)]
#[diesel(table_name = lost_found_items)]
pub struct LostFoundItem {
    pub id: Uuid,
    pub facility_id: Uuid,
    pub status: String,
    pub title: String,
    pub description: String,
    pub category: String,
    pub tags: Value,
    pub event_date: Option<NaiveDate>,
    pub event_time_text: Option<String>,
    pub location_text: String,
    pub bounce_reason: Option<String>,
    pub created_by: Uuid,
    pub created_at: NaiveDateTime,
    pub created_offset_minutes: i16,
    pub updated_at: NaiveDateTime,
    pub updated_offset_minutes: i16,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = lost_found_items)]
pub struct NewLostFoundItem {
    pub id: Uuid,
    pub facility_id: Uuid,
    pub status: String,
    pub title: String,
    pub description: String,
    pub category: String,
    pub tags: Value,
    pub event_date: Option<NaiveDate>,
    pub event_time_text: Option<String>,
    pub location_text: String,
    pub bounce_reason: Option<String>,
    pub created_by: Uuid,
    pub created_at: NaiveDateTime,
    pub created_offset_minutes: i16,
    pub updated_at: NaiveDateTime,
    pub updated_offset_minutes: i16,
}
