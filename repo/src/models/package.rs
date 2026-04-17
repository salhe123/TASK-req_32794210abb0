use bigdecimal::BigDecimal;
use chrono::NaiveDateTime;
use diesel::prelude::*;
use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

use crate::schema::{inventory_items, package_variants, packages, time_slots};

#[derive(Debug, Clone, Queryable, Identifiable, Serialize, AsChangeset)]
#[diesel(table_name = packages)]
pub struct Package {
    pub id: Uuid,
    pub facility_id: Uuid,
    pub name: String,
    pub description: String,
    pub base_price: BigDecimal,
    pub status: String,
    pub created_at: NaiveDateTime,
    pub created_offset_minutes: i16,
    pub updated_at: NaiveDateTime,
    pub updated_offset_minutes: i16,
    pub included_items: Value,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = packages)]
pub struct NewPackage {
    pub id: Uuid,
    pub facility_id: Uuid,
    pub name: String,
    pub description: String,
    pub base_price: BigDecimal,
    pub status: String,
    pub created_at: NaiveDateTime,
    pub created_offset_minutes: i16,
    pub updated_at: NaiveDateTime,
    pub updated_offset_minutes: i16,
    pub included_items: Value,
}

#[derive(Debug, Clone, Queryable, Identifiable, Serialize, AsChangeset)]
#[diesel(table_name = package_variants)]
pub struct PackageVariant {
    pub id: Uuid,
    pub package_id: Uuid,
    pub combination_key: String,
    pub price: BigDecimal,
    pub inventory_item_id: Option<Uuid>,
    pub time_slot_id: Option<Uuid>,
    pub created_at: NaiveDateTime,
    pub created_offset_minutes: i16,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = package_variants)]
pub struct NewPackageVariant {
    pub id: Uuid,
    pub package_id: Uuid,
    pub combination_key: String,
    pub price: BigDecimal,
    pub inventory_item_id: Option<Uuid>,
    pub time_slot_id: Option<Uuid>,
    pub created_at: NaiveDateTime,
    pub created_offset_minutes: i16,
}

#[derive(Debug, Clone, Queryable, Identifiable, Serialize)]
#[diesel(table_name = inventory_items)]
pub struct InventoryItem {
    pub id: Uuid,
    pub facility_id: Uuid,
    pub name: String,
    pub sku: String,
    pub created_at: NaiveDateTime,
    pub created_offset_minutes: i16,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = inventory_items)]
pub struct NewInventoryItem {
    pub id: Uuid,
    pub facility_id: Uuid,
    pub name: String,
    pub sku: String,
    pub created_at: NaiveDateTime,
    pub created_offset_minutes: i16,
}

#[derive(Debug, Clone, Queryable, Identifiable, Serialize)]
#[diesel(table_name = time_slots)]
pub struct TimeSlot {
    pub id: Uuid,
    pub facility_id: Uuid,
    pub starts_at: NaiveDateTime,
    pub starts_offset_minutes: i16,
    pub ends_at: NaiveDateTime,
    pub ends_offset_minutes: i16,
    pub created_at: NaiveDateTime,
    pub created_offset_minutes: i16,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = time_slots)]
pub struct NewTimeSlot {
    pub id: Uuid,
    pub facility_id: Uuid,
    pub starts_at: NaiveDateTime,
    pub starts_offset_minutes: i16,
    pub ends_at: NaiveDateTime,
    pub ends_offset_minutes: i16,
    pub created_at: NaiveDateTime,
    pub created_offset_minutes: i16,
}
