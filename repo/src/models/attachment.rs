use chrono::NaiveDateTime;
use diesel::prelude::*;
use serde::Serialize;
use uuid::Uuid;

use crate::schema::{attachment_blobs, attachments};

#[derive(Debug, Clone, Queryable, Identifiable, Serialize)]
#[diesel(primary_key(facility_id, sha256))]
#[diesel(table_name = attachment_blobs)]
pub struct AttachmentBlob {
    pub sha256: String,
    pub facility_id: Uuid,
    pub mime_type: String,
    pub size_bytes: i64,
    pub storage_path: String,
    pub ref_count: i32,
    pub created_at: NaiveDateTime,
    pub created_offset_minutes: i16,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = attachment_blobs)]
pub struct NewAttachmentBlob {
    pub sha256: String,
    pub facility_id: Uuid,
    pub mime_type: String,
    pub size_bytes: i64,
    pub storage_path: String,
    pub ref_count: i32,
    pub created_at: NaiveDateTime,
    pub created_offset_minutes: i16,
}

#[derive(Debug, Clone, Queryable, Identifiable, Serialize)]
#[diesel(table_name = attachments)]
pub struct Attachment {
    pub id: Uuid,
    pub facility_id: Uuid,
    pub parent_type: String,
    pub parent_id: Uuid,
    pub sha256: String,
    pub filename: String,
    pub mime_type: String,
    pub size_bytes: i64,
    pub created_by: Uuid,
    pub created_at: NaiveDateTime,
    pub created_offset_minutes: i16,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = attachments)]
pub struct NewAttachment {
    pub id: Uuid,
    pub facility_id: Uuid,
    pub parent_type: String,
    pub parent_id: Uuid,
    pub sha256: String,
    pub filename: String,
    pub mime_type: String,
    pub size_bytes: i64,
    pub created_by: Uuid,
    pub created_at: NaiveDateTime,
    pub created_offset_minutes: i16,
}
