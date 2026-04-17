use std::fs;
use std::io::Cursor;
use std::path::PathBuf;

use diesel::prelude::*;
use image::ImageFormat;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::db::DbPool;
use crate::errors::{AppError, AppResult};
use crate::models::attachment::{
    Attachment, AttachmentBlob, NewAttachment, NewAttachmentBlob,
};
use crate::schema::{attachment_blobs, attachments};
use crate::services::time::now_utc_naive;

pub const MAX_FILES_PER_PARENT: i64 = 10;
pub const MAX_BYTES_PER_PARENT: i64 = 25 * 1024 * 1024;
pub const MAX_LONG_EDGE_PX: u32 = 1920;

pub struct UploadRequest {
    pub facility_id: Uuid,
    pub parent_type: String,
    pub parent_id: Uuid,
    pub filename: String,
    pub mime_type: String,
    pub raw_bytes: Vec<u8>,
    pub created_by: Uuid,
}

pub struct UploadResult {
    pub attachment: Attachment,
    pub deduplicated: bool,
}

fn is_image_mime(m: &str) -> bool {
    matches!(m, "image/jpeg" | "image/png" | "image/webp")
}

fn accepted_mime(m: &str) -> bool {
    matches!(
        m,
        "image/jpeg" | "image/png" | "image/webp" | "application/pdf"
    )
}

fn resize_image(bytes: &[u8], mime: &str) -> AppResult<Vec<u8>> {
    let format = match mime {
        "image/jpeg" => ImageFormat::Jpeg,
        "image/png" => ImageFormat::Png,
        "image/webp" => ImageFormat::WebP,
        _ => return Err(AppError::InvalidAttachment("unsupported image mime".into())),
    };
    let img = image::load_from_memory_with_format(bytes, format)
        .map_err(|e| AppError::InvalidAttachment(format!("decode: {}", e)))?;
    let (w, h) = (img.width(), img.height());
    let long_edge = w.max(h);
    let final_img = if long_edge > MAX_LONG_EDGE_PX {
        let ratio = MAX_LONG_EDGE_PX as f32 / long_edge as f32;
        let nw = (w as f32 * ratio).round() as u32;
        let nh = (h as f32 * ratio).round() as u32;
        img.resize(nw, nh, image::imageops::FilterType::Lanczos3)
    } else {
        img
    };
    let mut out = Cursor::new(Vec::new());
    final_img
        .write_to(&mut out, format)
        .map_err(|e| AppError::InvalidAttachment(format!("encode: {}", e)))?;
    Ok(out.into_inner())
}

pub fn upload(pool: &DbPool, blob_dir: &str, req: UploadRequest) -> AppResult<UploadResult> {
    if !accepted_mime(&req.mime_type) {
        return Err(AppError::InvalidAttachment(format!(
            "unsupported mime type: {}",
            req.mime_type
        )));
    }

    let final_bytes = if is_image_mime(&req.mime_type) {
        resize_image(&req.raw_bytes, &req.mime_type)?
    } else {
        req.raw_bytes
    };
    let size = final_bytes.len() as i64;

    let mut conn = pool.get()?;

    let current_count: i64 = attachments::table
        .filter(attachments::parent_type.eq(&req.parent_type))
        .filter(attachments::parent_id.eq(&req.parent_id))
        .count()
        .get_result(&mut conn)?;
    if current_count + 1 > MAX_FILES_PER_PARENT {
        return Err(AppError::AttachmentLimit { limit: "files" });
    }
    let current_bytes: Option<i64> = attachments::table
        .filter(attachments::parent_type.eq(&req.parent_type))
        .filter(attachments::parent_id.eq(&req.parent_id))
        .select(diesel::dsl::sum(attachments::size_bytes))
        .first::<Option<bigdecimal::BigDecimal>>(&mut conn)?
        .and_then(|v| {
            use bigdecimal::ToPrimitive;
            v.to_i64()
        });
    let existing_bytes = current_bytes.unwrap_or(0);
    if existing_bytes + size > MAX_BYTES_PER_PARENT {
        return Err(AppError::AttachmentLimit { limit: "bytes" });
    }

    let mut hasher = Sha256::new();
    hasher.update(&final_bytes);
    let sha = hex::encode(hasher.finalize());

    let dir = PathBuf::from(blob_dir).join(req.facility_id.to_string());
    fs::create_dir_all(&dir)
        .map_err(|e| AppError::Internal(format!("blob dir: {}", e)))?;
    let storage_path = dir.join(&sha);
    let storage_path_str = storage_path.to_string_lossy().to_string();

    let existing_blob: Option<AttachmentBlob> = attachment_blobs::table
        .find((req.facility_id, sha.clone()))
        .first(&mut conn)
        .optional()?;

    let deduplicated = existing_blob.is_some();
    let (now, off) = now_utc_naive();

    if existing_blob.is_none() {
        fs::write(&storage_path, &final_bytes)
            .map_err(|e| AppError::Internal(format!("blob write: {}", e)))?;
        diesel::insert_into(attachment_blobs::table)
            .values(NewAttachmentBlob {
                sha256: sha.clone(),
                facility_id: req.facility_id,
                mime_type: req.mime_type.clone(),
                size_bytes: size,
                storage_path: storage_path_str,
                ref_count: 0,
                created_at: now,
                created_offset_minutes: off,
            })
            .execute(&mut conn)?;
    }

    diesel::update(
        attachment_blobs::table.find((req.facility_id, sha.clone())),
    )
    .set(attachment_blobs::ref_count.eq(attachment_blobs::ref_count + 1))
    .execute(&mut conn)?;

    let attachment = NewAttachment {
        id: Uuid::new_v4(),
        facility_id: req.facility_id,
        parent_type: req.parent_type,
        parent_id: req.parent_id,
        sha256: sha,
        filename: req.filename,
        mime_type: req.mime_type,
        size_bytes: size,
        created_by: req.created_by,
        created_at: now,
        created_offset_minutes: off,
    };
    let saved: Attachment = diesel::insert_into(attachments::table)
        .values(&attachment)
        .get_result(&mut conn)?;

    Ok(UploadResult {
        attachment: saved,
        deduplicated,
    })
}

/// Delete an attachment. The replay key is `(attachment_id, facility_id,
/// parent_type, parent_id)` — a caller cannot delete attachment A belonging
/// to item X by routing the request through item Y, even when both items
/// share the same facility. This closes the IDOR-within-facility reported
/// in audit round 3 / High #2.
pub fn delete(
    pool: &DbPool,
    attachment_id: Uuid,
    facility_id: Uuid,
    parent_type: &str,
    parent_id: Uuid,
) -> AppResult<()> {
    let mut conn = pool.get()?;
    let att: Attachment = attachments::table
        .filter(attachments::id.eq(attachment_id))
        .filter(attachments::facility_id.eq(facility_id))
        .filter(attachments::parent_type.eq(parent_type))
        .filter(attachments::parent_id.eq(parent_id))
        .first(&mut conn)
        .map_err(|e| match e {
            diesel::result::Error::NotFound => AppError::NotFound,
            other => other.into(),
        })?;
    diesel::delete(attachments::table.filter(attachments::id.eq(attachment_id)))
        .execute(&mut conn)?;
    diesel::update(attachment_blobs::table.find((att.facility_id, att.sha256.clone())))
        .set(attachment_blobs::ref_count.eq(attachment_blobs::ref_count - 1))
        .execute(&mut conn)?;
    let remaining: AttachmentBlob = attachment_blobs::table
        .find((att.facility_id, att.sha256.clone()))
        .first(&mut conn)?;
    if remaining.ref_count <= 0 {
        let _ = fs::remove_file(&remaining.storage_path);
        diesel::delete(
            attachment_blobs::table.find((att.facility_id, att.sha256)),
        )
        .execute(&mut conn)?;
    }
    Ok(())
}

pub fn list(pool: &DbPool, parent_type: &str, parent_id: Uuid) -> AppResult<Vec<Attachment>> {
    let mut conn = pool.get()?;
    let rows: Vec<Attachment> = attachments::table
        .filter(attachments::parent_type.eq(parent_type))
        .filter(attachments::parent_id.eq(parent_id))
        .order(attachments::created_at.asc())
        .load(&mut conn)?;
    Ok(rows)
}
