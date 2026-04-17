use diesel::prelude::*;
use serde_json::Value;
use uuid::Uuid;

use crate::db::DbPool;
use crate::errors::AppResult;
use crate::models::audit::NewAuditLog;
use crate::schema::audit_logs;
use crate::services::time::now_utc_naive;

pub struct AuditEntry {
    pub actor_user_id: Option<Uuid>,
    pub facility_id: Option<Uuid>,
    pub entity_type: String,
    pub entity_id: Uuid,
    pub action: String,
    pub before_state: Option<Value>,
    pub after_state: Option<Value>,
    pub request_id: Option<String>,
}

pub fn write(pool: &DbPool, entry: AuditEntry) -> AppResult<()> {
    let mut conn = pool.get()?;
    let (now, off) = now_utc_naive();
    let new = NewAuditLog {
        id: Uuid::new_v4(),
        actor_user_id: entry.actor_user_id,
        facility_id: entry.facility_id,
        entity_type: entry.entity_type,
        entity_id: entry.entity_id,
        action: entry.action,
        before_state: entry.before_state,
        after_state: entry.after_state,
        request_id: entry.request_id,
        created_at: now,
        created_offset_minutes: off,
    };
    diesel::insert_into(audit_logs::table)
        .values(&new)
        .execute(&mut conn)?;
    Ok(())
}
