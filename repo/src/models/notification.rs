use chrono::NaiveDateTime;
use diesel::prelude::*;
use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

use crate::schema::{
    notification_subscriptions, notification_templates, notifications, outbox_deliveries,
};

#[derive(Debug, Clone, Queryable, Identifiable, Serialize, AsChangeset)]
#[diesel(table_name = notification_templates)]
pub struct NotificationTemplate {
    pub id: Uuid,
    pub code: String,
    pub subject: String,
    pub body: String,
    pub is_active: bool,
    pub created_at: NaiveDateTime,
    pub created_offset_minutes: i16,
    pub updated_at: NaiveDateTime,
    pub updated_offset_minutes: i16,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = notification_templates)]
pub struct NewNotificationTemplate {
    pub id: Uuid,
    pub code: String,
    pub subject: String,
    pub body: String,
    pub is_active: bool,
    pub created_at: NaiveDateTime,
    pub created_offset_minutes: i16,
    pub updated_at: NaiveDateTime,
    pub updated_offset_minutes: i16,
}

#[derive(Debug, Clone, Queryable, Identifiable, Serialize)]
#[diesel(table_name = notifications)]
pub struct Notification {
    pub id: Uuid,
    pub user_id: Uuid,
    pub event_kind: String,
    pub subject: String,
    pub body: String,
    pub payload: Value,
    pub is_read: bool,
    pub read_at: Option<NaiveDateTime>,
    pub created_at: NaiveDateTime,
    pub created_offset_minutes: i16,
    pub read_offset_minutes: Option<i16>,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = notifications)]
pub struct NewNotification {
    pub id: Uuid,
    pub user_id: Uuid,
    pub event_kind: String,
    pub subject: String,
    pub body: String,
    pub payload: Value,
    pub is_read: bool,
    pub read_at: Option<NaiveDateTime>,
    pub created_at: NaiveDateTime,
    pub created_offset_minutes: i16,
    pub read_offset_minutes: Option<i16>,
}

#[derive(Debug, Clone, Queryable, Identifiable, Serialize, AsChangeset)]
#[diesel(table_name = outbox_deliveries)]
pub struct OutboxDelivery {
    pub id: Uuid,
    pub user_id: Uuid,
    pub event_kind: String,
    pub template_code: String,
    pub subject: String,
    pub body: String,
    pub payload: Value,
    pub status: String,
    pub attempt_count: i32,
    pub next_attempt_at: Option<NaiveDateTime>,
    pub last_error: Option<String>,
    pub created_at: NaiveDateTime,
    pub created_offset_minutes: i16,
    pub updated_at: NaiveDateTime,
    pub updated_offset_minutes: i16,
    pub channel: String,
    pub to_address: Option<String>,
    pub facility_id: Option<Uuid>,
    pub next_attempt_offset_minutes: Option<i16>,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = outbox_deliveries)]
pub struct NewOutboxDelivery {
    pub id: Uuid,
    pub user_id: Uuid,
    pub event_kind: String,
    pub template_code: String,
    pub subject: String,
    pub body: String,
    pub payload: Value,
    pub status: String,
    pub attempt_count: i32,
    pub next_attempt_at: Option<NaiveDateTime>,
    pub last_error: Option<String>,
    pub created_at: NaiveDateTime,
    pub created_offset_minutes: i16,
    pub updated_at: NaiveDateTime,
    pub updated_offset_minutes: i16,
    pub channel: String,
    pub to_address: Option<String>,
    pub facility_id: Option<Uuid>,
    pub next_attempt_offset_minutes: Option<i16>,
}

#[derive(Debug, Clone, Queryable, Identifiable, Serialize, AsChangeset)]
#[diesel(primary_key(user_id, event_kind))]
#[diesel(table_name = notification_subscriptions)]
pub struct NotificationSubscription {
    pub user_id: Uuid,
    pub event_kind: String,
    pub enabled: bool,
    pub updated_at: NaiveDateTime,
    pub updated_offset_minutes: i16,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = notification_subscriptions)]
pub struct NewNotificationSubscription {
    pub user_id: Uuid,
    pub event_kind: String,
    pub enabled: bool,
    pub updated_at: NaiveDateTime,
    pub updated_offset_minutes: i16,
}
