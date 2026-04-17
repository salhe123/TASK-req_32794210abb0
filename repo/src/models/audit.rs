use chrono::NaiveDateTime;
use diesel::prelude::*;
use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

use crate::schema::audit_logs;

#[derive(Debug, Clone, Queryable, Identifiable, Serialize)]
#[diesel(table_name = audit_logs)]
pub struct AuditLog {
    pub id: Uuid,
    pub actor_user_id: Option<Uuid>,
    pub facility_id: Option<Uuid>,
    pub entity_type: String,
    pub entity_id: Uuid,
    pub action: String,
    pub before_state: Option<Value>,
    pub after_state: Option<Value>,
    pub request_id: Option<String>,
    pub created_at: NaiveDateTime,
    pub created_offset_minutes: i16,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = audit_logs)]
pub struct NewAuditLog {
    pub id: Uuid,
    pub actor_user_id: Option<Uuid>,
    pub facility_id: Option<Uuid>,
    pub entity_type: String,
    pub entity_id: Uuid,
    pub action: String,
    pub before_state: Option<Value>,
    pub after_state: Option<Value>,
    pub request_id: Option<String>,
    pub created_at: NaiveDateTime,
    pub created_offset_minutes: i16,
}
