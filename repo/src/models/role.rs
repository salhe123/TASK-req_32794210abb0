use chrono::NaiveDateTime;
use diesel::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::schema::{permissions, role_permissions, roles, user_roles};

#[derive(Debug, Clone, Queryable, Identifiable, Serialize)]
#[diesel(table_name = roles)]
pub struct Role {
    pub id: Uuid,
    pub name: String,
    pub data_scope: String,
    pub field_allowlist: Value,
    pub created_at: NaiveDateTime,
    pub created_offset_minutes: i16,
}

#[derive(Debug, Insertable, Deserialize)]
#[diesel(table_name = roles)]
pub struct NewRole {
    pub id: Uuid,
    pub name: String,
    pub data_scope: String,
    pub field_allowlist: Value,
    pub created_at: NaiveDateTime,
    pub created_offset_minutes: i16,
}

#[derive(Debug, Clone, Queryable, Identifiable, Serialize)]
#[diesel(table_name = permissions)]
pub struct Permission {
    pub id: Uuid,
    pub code: String,
    pub description: String,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = permissions)]
pub struct NewPermission {
    pub id: Uuid,
    pub code: String,
    pub description: String,
}

#[derive(Debug, Insertable, Queryable)]
#[diesel(table_name = role_permissions)]
pub struct RolePermission {
    pub role_id: Uuid,
    pub permission_id: Uuid,
}

#[derive(Debug, Insertable, Queryable)]
#[diesel(table_name = user_roles)]
pub struct UserRole {
    pub user_id: Uuid,
    pub role_id: Uuid,
}
