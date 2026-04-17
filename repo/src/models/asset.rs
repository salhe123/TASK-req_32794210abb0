use chrono::NaiveDateTime;
use diesel::prelude::*;
use serde::Serialize;
use uuid::Uuid;

use crate::schema::{asset_events, assets, maintenance_records};

#[derive(Debug, Clone, Queryable, Identifiable, Serialize, AsChangeset)]
#[diesel(table_name = assets)]
pub struct Asset {
    pub id: Uuid,
    pub facility_id: Uuid,
    pub asset_label: String,
    pub name: String,
    pub status: String,
    pub prior_status: Option<String>,
    pub description: String,
    pub acquired_at: Option<NaiveDateTime>,
    pub acquired_offset_minutes: Option<i16>,
    pub created_at: NaiveDateTime,
    pub created_offset_minutes: i16,
    pub updated_at: NaiveDateTime,
    pub updated_offset_minutes: i16,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = assets)]
pub struct NewAsset {
    pub id: Uuid,
    pub facility_id: Uuid,
    pub asset_label: String,
    pub name: String,
    pub status: String,
    pub prior_status: Option<String>,
    pub description: String,
    pub acquired_at: Option<NaiveDateTime>,
    pub acquired_offset_minutes: Option<i16>,
    pub created_at: NaiveDateTime,
    pub created_offset_minutes: i16,
    pub updated_at: NaiveDateTime,
    pub updated_offset_minutes: i16,
}

#[derive(Debug, Clone, Queryable, Identifiable, Serialize)]
#[diesel(table_name = asset_events)]
pub struct AssetEvent {
    pub id: Uuid,
    pub asset_id: Uuid,
    pub from_status: Option<String>,
    pub to_status: String,
    pub actor_user_id: Uuid,
    pub note: String,
    pub created_at: NaiveDateTime,
    pub created_offset_minutes: i16,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = asset_events)]
pub struct NewAssetEvent {
    pub id: Uuid,
    pub asset_id: Uuid,
    pub from_status: Option<String>,
    pub to_status: String,
    pub actor_user_id: Uuid,
    pub note: String,
    pub created_at: NaiveDateTime,
    pub created_offset_minutes: i16,
}

#[derive(Debug, Clone, Queryable, Identifiable, Serialize)]
#[diesel(table_name = maintenance_records)]
pub struct MaintenanceRecord {
    pub id: Uuid,
    pub asset_id: Uuid,
    pub performed_at: NaiveDateTime,
    pub performed_offset_minutes: i16,
    pub performed_by: Uuid,
    pub summary: String,
    pub details: String,
    pub created_at: NaiveDateTime,
    pub created_offset_minutes: i16,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = maintenance_records)]
pub struct NewMaintenanceRecord {
    pub id: Uuid,
    pub asset_id: Uuid,
    pub performed_at: NaiveDateTime,
    pub performed_offset_minutes: i16,
    pub performed_by: Uuid,
    pub summary: String,
    pub details: String,
    pub created_at: NaiveDateTime,
    pub created_offset_minutes: i16,
}
