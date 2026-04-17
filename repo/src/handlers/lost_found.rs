use actix_web::dev::HttpServiceFactory;
use actix_web::{web, HttpMessage, HttpRequest, HttpResponse};
use base64::Engine;
use diesel::prelude::*;
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::config::Config;
use crate::db::DbPool;
use crate::errors::{AppError, AppResult};
use crate::middleware::idempotency;
use crate::middleware::request_context::RequestContext;
use crate::models::lost_found::{LostFoundItem, NewLostFoundItem};
use crate::schema::{audit_logs, lost_found_items, stores};
use crate::services::attachments as att_svc;
use crate::services::audit as audit_svc;
use crate::services::notify;
use crate::services::time::now_utc_naive;

pub const PARENT_TYPE: &str = "lost_found_item";

pub const STATUS_DRAFT: &str = "DRAFT";
pub const STATUS_IN_REVIEW: &str = "IN_REVIEW";
pub const STATUS_PUBLISHED: &str = "PUBLISHED";
pub const STATUS_UNPUBLISHED: &str = "UNPUBLISHED";
pub const STATUS_DELETED: &str = "DELETED";

pub const MAX_LOCATION_TEXT: usize = 200;

/// Standardized lost-and-found occurrence/category set.
/// See `migrations/2026-01-01-000008_audit_fixes/up.sql` for the DB-side CHECK.
pub const ALLOWED_CATEGORIES: &[&str] = &["lost", "found", "returned", "damaged", "other"];

fn validate_category(cat: &str) -> AppResult<()> {
    let trimmed = cat.trim();
    if !ALLOWED_CATEGORIES.contains(&trimmed) {
        return Err(AppError::Validation {
            message: "category must be one of the allowed values".into(),
            details: json!({ "field": "category", "allowed": ALLOWED_CATEGORIES }),
        });
    }
    Ok(())
}

fn validate_location(loc: &str) -> AppResult<()> {
    if loc.chars().count() > MAX_LOCATION_TEXT {
        return Err(AppError::Validation {
            message: "locationText must be at most 200 characters".into(),
            details: json!({ "field": "locationText", "limit": MAX_LOCATION_TEXT }),
        });
    }
    Ok(())
}

fn validate_event_time(et: &str) -> AppResult<()> {
    if crate::services::time::parse_time_12h(et).is_none() {
        return Err(AppError::Validation {
            message: "eventTime must be 12-hour with AM/PM".into(),
            details: json!({ "field": "eventTime" }),
        });
    }
    Ok(())
}

pub fn scope() -> impl HttpServiceFactory {
    web::scope("/lost-found")
        .wrap(crate::middleware::auth::Authenticate)
        .service(
            web::resource("/items")
                .route(web::post().to(create_item))
                .route(web::get().to(list_items)),
        )
        .service(
            web::resource("/items/{id}")
                .route(web::get().to(get_item))
                .route(web::put().to(update_item))
                .route(web::delete().to(delete_item)),
        )
        .route("/items/{id}/submit", web::post().to(submit_item))
        .route("/items/{id}/approve", web::post().to(approve_item))
        .route("/items/{id}/bounce", web::post().to(bounce_item))
        .route("/items/{id}/unpublish", web::post().to(unpublish_item))
        .route("/items/{id}/republish", web::post().to(republish_item))
        .route("/items/{id}/history", web::get().to(item_history))
        .service(
            web::resource("/items/{id}/attachments")
                .route(web::get().to(list_attachments))
                .route(web::post().to(upload_attachment)),
        )
        .route(
            "/items/{id}/attachments/{attachmentId}",
            web::delete().to(delete_attachment),
        )
}

fn require_ctx(req: &HttpRequest) -> AppResult<RequestContext> {
    let ext = req.extensions();
    ext.get::<RequestContext>()
        .cloned()
        .ok_or(AppError::Unauthenticated)
}

fn enforce_facility_scope(ctx: &RequestContext, facility_id: Uuid) -> AppResult<()> {
    match ctx.allowed_facilities() {
        None => Ok(()),
        Some(set) if set.contains(&facility_id) => Ok(()),
        _ => Err(AppError::OutOfScope),
    }
}

fn serialize_item(item: &LostFoundItem) -> Value {
    json!({
        "id": item.id,
        "facilityId": item.facility_id,
        "status": item.status,
        "title": item.title,
        "description": item.description,
        "category": item.category,
        "tags": item.tags,
        "eventDate": item.event_date.as_ref().map(|d| d.format("%m/%d/%Y").to_string()),
        "eventTime": item.event_time_text,
        "locationText": item.location_text,
        "bounceReason": item.bounce_reason,
        "deleted": item.status == STATUS_DELETED,
        "createdBy": item.created_by,
        "createdAt": item.created_at.format("%Y-%m-%dT%H:%M:%S").to_string(),
        "updatedAt": item.updated_at.format("%Y-%m-%dT%H:%M:%S").to_string(),
    })
}

#[derive(Debug, Deserialize)]
struct CreateBody {
    #[serde(rename = "facilityId")]
    facility_id: Uuid,
    title: String,
    #[serde(default)]
    description: String,
    category: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(rename = "eventDate")]
    event_date: Option<String>,
    #[serde(rename = "eventTime")]
    event_time: Option<String>,
    #[serde(rename = "locationText", default)]
    location_text: String,
}

fn validate_tags(tags: &[String]) -> AppResult<()> {
    if tags.len() > 10 {
        return Err(AppError::Validation {
            message: "at most 10 tags allowed".into(),
            details: json!({ "field": "tags", "limit": 10 }),
        });
    }
    for t in tags {
        let n = t.chars().count();
        if !(2..=24).contains(&n) {
            return Err(AppError::Validation {
                message: "tag length must be 2..=24 characters".into(),
                details: json!({ "field": "tags", "value": t }),
            });
        }
    }
    Ok(())
}

fn validate_for_submission(item: &LostFoundItem) -> AppResult<()> {
    if item.event_date.is_none() {
        return Err(AppError::Validation {
            message: "eventDate required to submit".into(),
            details: json!({ "field": "eventDate" }),
        });
    }
    let t = item.event_time_text.as_deref().unwrap_or("");
    validate_event_time(t)?;
    validate_location(&item.location_text)?;
    validate_category(&item.category)?;
    if let Some(arr) = item.tags.as_array() {
        let tags: Vec<String> = arr
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect();
        validate_tags(&tags)?;
    }
    Ok(())
}

fn assert_facility_exists(conn: &mut diesel::PgConnection, facility_id: Uuid) -> AppResult<()> {
    let exists: Option<Uuid> = stores::table
        .filter(stores::id.eq(facility_id))
        .select(stores::id)
        .first(conn)
        .optional()?;
    if exists.is_none() {
        return Err(AppError::Validation {
            message: "facility not found".into(),
            details: json!({ "field": "facilityId" }),
        });
    }
    Ok(())
}

async fn create_item(
    req: HttpRequest,
    pool: web::Data<DbPool>,
    body: web::Json<CreateBody>,
) -> AppResult<HttpResponse> {
    let ctx = require_ctx(&req)?;
    if !ctx.has_any_permission(&["lost_found.edit_draft"]) {
        return Err(AppError::Forbidden);
    }
    enforce_facility_scope(&ctx, body.facility_id)?;
    validate_tags(&body.tags)?;
    validate_category(&body.category)?;
    validate_location(&body.location_text)?;

    if body.title.trim().is_empty() {
        return Err(AppError::Validation {
            message: "title required".into(),
            details: json!({ "field": "title" }),
        });
    }
    let event_date = match &body.event_date {
        Some(s) if !s.is_empty() => {
            crate::services::time::parse_date_mdy(s).ok_or_else(|| AppError::Validation {
                message: "eventDate must be MM/DD/YYYY".into(),
                details: json!({ "field": "eventDate" }),
            })?;
            crate::services::time::parse_date_mdy(s)
        }
        _ => None,
    };

    if let Some(et) = &body.event_time {
        if !et.is_empty() {
            validate_event_time(et)?;
        }
    }

    if let Some(replay) = idempotency::check_before(pool.get_ref(), &ctx, &req)? {
        return Ok(replay);
    }

    let mut conn = pool.get()?;
    assert_facility_exists(&mut conn, body.facility_id)?;
    let (now, off) = now_utc_naive();
    let new = NewLostFoundItem {
        id: Uuid::new_v4(),
        facility_id: body.facility_id,
        status: STATUS_DRAFT.to_string(),
        title: body.title.trim().to_string(),
        description: body.description.clone(),
        category: body.category.clone(),
        tags: json!(body.tags),
        event_date,
        event_time_text: body.event_time.clone(),
        location_text: body.location_text.clone(),
        bounce_reason: None,
        created_by: ctx.user.id,
        created_at: now,
        created_offset_minutes: off,
        updated_at: now,
        updated_offset_minutes: off,
    };
    let saved: LostFoundItem = diesel::insert_into(lost_found_items::table)
        .values(&new)
        .get_result(&mut conn)?;
    audit_svc::write(
        pool.get_ref(),
        audit_svc::AuditEntry {
            actor_user_id: Some(ctx.user.id),
            facility_id: Some(saved.facility_id),
            entity_type: PARENT_TYPE.into(),
            entity_id: saved.id,
            action: "create".into(),
            before_state: None,
            after_state: Some(serialize_item(&saved)),
            request_id: ctx.request_id.clone(),
        },
    )?;
    let body_json = serialize_item(&saved);
    idempotency::record_after(
        pool.get_ref(),
        &ctx,
        "POST",
        "/api/lost-found/items",
        201,
        &body_json,
    )?;
    Ok(HttpResponse::Created().json(body_json))
}

#[derive(Debug, Deserialize)]
struct ListQuery {
    #[serde(rename = "facilityId")]
    facility_id: Option<Uuid>,
    status: Option<String>,
    #[serde(default, rename = "includeDeleted")]
    include_deleted: bool,
}

async fn list_items(
    req: HttpRequest,
    pool: web::Data<DbPool>,
    q: web::Query<ListQuery>,
) -> AppResult<HttpResponse> {
    let ctx = require_ctx(&req)?;
    if !ctx.has_any_permission(&["lost_found.read", "lost_found.edit_draft", "lost_found.review"]) {
        return Err(AppError::Forbidden);
    }
    let mut conn = pool.get()?;
    let mut query = lost_found_items::table.into_boxed();
    match ctx.allowed_facilities() {
        None => {}
        Some(set) => {
            let ids: Vec<Uuid> = set.into_iter().collect();
            query = query.filter(lost_found_items::facility_id.eq_any(ids));
        }
    }
    if let Some(fid) = q.facility_id {
        enforce_facility_scope(&ctx, fid)?;
        query = query.filter(lost_found_items::facility_id.eq(fid));
    }
    if let Some(s) = &q.status {
        query = query.filter(lost_found_items::status.eq(s));
    }
    if !q.include_deleted {
        query = query.filter(lost_found_items::status.ne(STATUS_DELETED));
    }
    let rows: Vec<LostFoundItem> = query
        .order(lost_found_items::created_at.desc())
        .limit(500)
        .load(&mut conn)?;
    let out: Vec<Value> = rows.iter().map(serialize_item).collect();
    Ok(HttpResponse::Ok().json(json!({ "items": out, "count": out.len() })))
}

async fn load_item(pool: &DbPool, id: Uuid) -> AppResult<LostFoundItem> {
    let mut conn = pool.get()?;
    lost_found_items::table
        .filter(lost_found_items::id.eq(id))
        .first::<LostFoundItem>(&mut conn)
        .map_err(|e| match e {
            diesel::result::Error::NotFound => AppError::NotFound,
            other => other.into(),
        })
}

async fn get_item(
    req: HttpRequest,
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
) -> AppResult<HttpResponse> {
    let ctx = require_ctx(&req)?;
    if !ctx.has_any_permission(&["lost_found.read", "lost_found.edit_draft", "lost_found.review"]) {
        return Err(AppError::Forbidden);
    }
    let item = load_item(pool.get_ref(), path.into_inner()).await?;
    enforce_facility_scope(&ctx, item.facility_id)?;
    Ok(HttpResponse::Ok().json(serialize_item(&item)))
}

#[derive(Debug, Deserialize)]
struct UpdateBody {
    title: Option<String>,
    description: Option<String>,
    category: Option<String>,
    tags: Option<Vec<String>>,
    #[serde(rename = "eventDate")]
    event_date: Option<String>,
    #[serde(rename = "eventTime")]
    event_time: Option<String>,
    #[serde(rename = "locationText")]
    location_text: Option<String>,
}

async fn update_item(
    req: HttpRequest,
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
    body: web::Json<UpdateBody>,
) -> AppResult<HttpResponse> {
    let ctx = require_ctx(&req)?;
    if !ctx.has_any_permission(&["lost_found.edit_draft"]) {
        return Err(AppError::Forbidden);
    }
    let id = path.into_inner();
    let existing = load_item(pool.get_ref(), id).await?;
    enforce_facility_scope(&ctx, existing.facility_id)?;
    if existing.status != STATUS_DRAFT {
        return Err(AppError::InvalidTransition(
            "only DRAFT items are editable".into(),
        ));
    }
    if existing.status == STATUS_DELETED {
        return Err(AppError::NotFound);
    }
    if let Some(tags) = &body.tags {
        validate_tags(tags)?;
    }
    if let Some(cat) = &body.category {
        validate_category(cat)?;
    }
    if let Some(loc) = &body.location_text {
        validate_location(loc)?;
    }
    if let Some(et) = &body.event_time {
        if !et.is_empty() {
            validate_event_time(et)?;
        }
    }
    if let Some(replay) = idempotency::check_before(pool.get_ref(), &ctx, &req)? {
        return Ok(replay);
    }
    let mut conn = pool.get()?;
    let (now, off) = now_utc_naive();
    let before = serialize_item(&existing);

    let new_event_date = match &body.event_date {
        Some(s) if !s.is_empty() => {
            Some(crate::services::time::parse_date_mdy(s).ok_or_else(|| {
                AppError::Validation {
                    message: "eventDate must be MM/DD/YYYY".into(),
                    details: json!({ "field": "eventDate" }),
                }
            })?)
        }
        _ => existing.event_date,
    };

    let tags_value = match &body.tags {
        Some(v) => json!(v),
        None => existing.tags.clone(),
    };

    let title_new = body.title.clone().unwrap_or_else(|| existing.title.clone());
    let desc_new = body
        .description
        .clone()
        .unwrap_or_else(|| existing.description.clone());
    let cat_new = body
        .category
        .clone()
        .unwrap_or_else(|| existing.category.clone());
    let et_new = body.event_time.clone().or(existing.event_time_text.clone());
    let loc_new = body
        .location_text
        .clone()
        .unwrap_or_else(|| existing.location_text.clone());

    diesel::update(lost_found_items::table.filter(lost_found_items::id.eq(id)))
        .set((
            lost_found_items::title.eq(&title_new),
            lost_found_items::description.eq(&desc_new),
            lost_found_items::category.eq(&cat_new),
            lost_found_items::tags.eq(&tags_value),
            lost_found_items::event_date.eq(new_event_date),
            lost_found_items::event_time_text.eq(et_new.clone()),
            lost_found_items::location_text.eq(&loc_new),
            lost_found_items::updated_at.eq(now),
            lost_found_items::updated_offset_minutes.eq(off),
        ))
        .execute(&mut conn)?;
    let updated = load_item(pool.get_ref(), id).await?;
    audit_svc::write(
        pool.get_ref(),
        audit_svc::AuditEntry {
            actor_user_id: Some(ctx.user.id),
            facility_id: Some(updated.facility_id),
            entity_type: PARENT_TYPE.into(),
            entity_id: updated.id,
            action: "update".into(),
            before_state: Some(before),
            after_state: Some(serialize_item(&updated)),
            request_id: ctx.request_id.clone(),
        },
    )?;
    let response = serialize_item(&updated);
    idempotency::record_after(
        pool.get_ref(),
        &ctx,
        "PUT",
        &format!("/api/lost-found/items/{}", id),
        200,
        &response,
    )?;
    Ok(HttpResponse::Ok().json(response))
}

async fn submit_item(
    req: HttpRequest,
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
) -> AppResult<HttpResponse> {
    let ctx = require_ctx(&req)?;
    if !ctx.has_any_permission(&["lost_found.edit_draft"]) {
        return Err(AppError::Forbidden);
    }
    let id = path.into_inner();
    let existing = load_item(pool.get_ref(), id).await?;
    enforce_facility_scope(&ctx, existing.facility_id)?;
    if existing.status == STATUS_DELETED {
        return Err(AppError::NotFound);
    }
    if existing.status != STATUS_DRAFT {
        return Err(AppError::InvalidTransition(format!(
            "can only submit from DRAFT, current={}",
            existing.status
        )));
    }
    validate_for_submission(&existing)?;
    if let Some(replay) = idempotency::check_before(pool.get_ref(), &ctx, &req)? {
        return Ok(replay);
    }
    transition(
        pool.get_ref(),
        &ctx,
        &existing,
        STATUS_IN_REVIEW,
        None,
        "submit",
    )
    .await?;
    // Fire notification for desk reviewers
    let _ = notify::enqueue(
        pool.get_ref(),
        notify::Trigger {
            user_id: ctx.user.id,
            event_kind: notify::EVENT_SUBMISSION,
            template_code: "lost_found.submitted".into(),
            facility_id: Some(existing.facility_id),
            channel: notify::CHANNEL_IN_APP,
            to_address: None,
            fallback_subject: format!("Item submitted: {}", existing.title),
            fallback_body: "A lost-and-found item was submitted for review.".into(),
            payload: json!({
                "item": { "id": existing.id, "title": existing.title, "status": "IN_REVIEW" }
            }),
        },
    );
    let updated = load_item(pool.get_ref(), id).await?;
    let response = serialize_item(&updated);
    idempotency::record_after(
        pool.get_ref(),
        &ctx,
        "POST",
        &format!("/api/lost-found/items/{}/submit", id),
        200,
        &response,
    )?;
    Ok(HttpResponse::Ok().json(response))
}

async fn approve_item(
    req: HttpRequest,
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
) -> AppResult<HttpResponse> {
    let ctx = require_ctx(&req)?;
    if !ctx.has_any_permission(&["lost_found.review"]) {
        return Err(AppError::Forbidden);
    }
    let id = path.into_inner();
    let existing = load_item(pool.get_ref(), id).await?;
    enforce_facility_scope(&ctx, existing.facility_id)?;
    if existing.status != STATUS_IN_REVIEW {
        return Err(AppError::InvalidTransition(format!(
            "can only approve from IN_REVIEW, current={}",
            existing.status
        )));
    }
    if let Some(replay) = idempotency::check_before(pool.get_ref(), &ctx, &req)? {
        return Ok(replay);
    }
    transition(
        pool.get_ref(),
        &ctx,
        &existing,
        STATUS_PUBLISHED,
        None,
        "approve",
    )
    .await?;
    let _ = notify::enqueue(
        pool.get_ref(),
        notify::Trigger {
            user_id: existing.created_by,
            event_kind: notify::EVENT_REVIEW,
            template_code: "lost_found.approved".into(),
            facility_id: Some(existing.facility_id),
            channel: notify::CHANNEL_IN_APP,
            to_address: None,
            fallback_subject: format!("Item approved: {}", existing.title),
            fallback_body: "Your lost-and-found item was published.".into(),
            payload: json!({ "item": { "id": existing.id, "title": existing.title, "status": "PUBLISHED" } }),
        },
    );
    let updated = load_item(pool.get_ref(), id).await?;
    let response = serialize_item(&updated);
    idempotency::record_after(
        pool.get_ref(),
        &ctx,
        "POST",
        &format!("/api/lost-found/items/{}/approve", id),
        200,
        &response,
    )?;
    Ok(HttpResponse::Ok().json(response))
}

#[derive(Debug, Deserialize)]
struct BounceBody {
    reason: String,
}

async fn bounce_item(
    req: HttpRequest,
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
    body: web::Json<BounceBody>,
) -> AppResult<HttpResponse> {
    let ctx = require_ctx(&req)?;
    if !ctx.has_any_permission(&["lost_found.review"]) {
        return Err(AppError::Forbidden);
    }
    if body.reason.trim().is_empty() {
        return Err(AppError::Validation {
            message: "reason is required to bounce".into(),
            details: json!({ "field": "reason" }),
        });
    }
    let id = path.into_inner();
    let existing = load_item(pool.get_ref(), id).await?;
    enforce_facility_scope(&ctx, existing.facility_id)?;
    if existing.status != STATUS_IN_REVIEW {
        return Err(AppError::InvalidTransition(format!(
            "can only bounce from IN_REVIEW, current={}",
            existing.status
        )));
    }
    if let Some(replay) = idempotency::check_before(pool.get_ref(), &ctx, &req)? {
        return Ok(replay);
    }
    transition(
        pool.get_ref(),
        &ctx,
        &existing,
        STATUS_DRAFT,
        Some(body.reason.clone()),
        "bounce",
    )
    .await?;
    let _ = notify::enqueue(
        pool.get_ref(),
        notify::Trigger {
            user_id: existing.created_by,
            event_kind: notify::EVENT_REVIEW,
            template_code: "lost_found.bounced".into(),
            facility_id: Some(existing.facility_id),
            channel: notify::CHANNEL_IN_APP,
            to_address: None,
            fallback_subject: format!("Item bounced: {}", existing.title),
            fallback_body: format!("Your lost-and-found submission was bounced: {}", body.reason),
            payload: json!({
                "item": { "id": existing.id, "title": existing.title, "status": "DRAFT", "bounceReason": body.reason }
            }),
        },
    );
    let updated = load_item(pool.get_ref(), id).await?;
    let response = serialize_item(&updated);
    idempotency::record_after(
        pool.get_ref(),
        &ctx,
        "POST",
        &format!("/api/lost-found/items/{}/bounce", id),
        200,
        &response,
    )?;
    Ok(HttpResponse::Ok().json(response))
}

async fn unpublish_item(
    req: HttpRequest,
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
) -> AppResult<HttpResponse> {
    let ctx = require_ctx(&req)?;
    if !ctx.has_any_permission(&["lost_found.review", "lost_found.edit_draft"]) {
        return Err(AppError::Forbidden);
    }
    let id = path.into_inner();
    let existing = load_item(pool.get_ref(), id).await?;
    enforce_facility_scope(&ctx, existing.facility_id)?;
    if existing.status != STATUS_PUBLISHED {
        return Err(AppError::InvalidTransition(format!(
            "can only unpublish from PUBLISHED, current={}",
            existing.status
        )));
    }
    if let Some(replay) = idempotency::check_before(pool.get_ref(), &ctx, &req)? {
        return Ok(replay);
    }
    transition(
        pool.get_ref(),
        &ctx,
        &existing,
        STATUS_UNPUBLISHED,
        None,
        "unpublish",
    )
    .await?;
    let updated = load_item(pool.get_ref(), id).await?;
    let response = serialize_item(&updated);
    idempotency::record_after(
        pool.get_ref(),
        &ctx,
        "POST",
        &format!("/api/lost-found/items/{}/unpublish", id),
        200,
        &response,
    )?;
    Ok(HttpResponse::Ok().json(response))
}

async fn republish_item(
    req: HttpRequest,
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
) -> AppResult<HttpResponse> {
    let ctx = require_ctx(&req)?;
    if !ctx.has_any_permission(&["lost_found.review", "lost_found.edit_draft"]) {
        return Err(AppError::Forbidden);
    }
    let id = path.into_inner();
    let existing = load_item(pool.get_ref(), id).await?;
    enforce_facility_scope(&ctx, existing.facility_id)?;
    if existing.status != STATUS_UNPUBLISHED {
        return Err(AppError::InvalidTransition(format!(
            "can only republish from UNPUBLISHED, current={}",
            existing.status
        )));
    }
    if let Some(replay) = idempotency::check_before(pool.get_ref(), &ctx, &req)? {
        return Ok(replay);
    }
    transition(
        pool.get_ref(),
        &ctx,
        &existing,
        STATUS_PUBLISHED,
        None,
        "republish",
    )
    .await?;
    let updated = load_item(pool.get_ref(), id).await?;
    let response = serialize_item(&updated);
    idempotency::record_after(
        pool.get_ref(),
        &ctx,
        "POST",
        &format!("/api/lost-found/items/{}/republish", id),
        200,
        &response,
    )?;
    Ok(HttpResponse::Ok().json(response))
}

async fn delete_item(
    req: HttpRequest,
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
) -> AppResult<HttpResponse> {
    let ctx = require_ctx(&req)?;
    if !ctx.has_any_permission(&["lost_found.edit_draft", "lost_found.review"]) {
        return Err(AppError::Forbidden);
    }
    let id = path.into_inner();
    let existing = load_item(pool.get_ref(), id).await?;
    enforce_facility_scope(&ctx, existing.facility_id)?;
    if existing.status == STATUS_DELETED {
        return Err(AppError::NotFound);
    }
    if let Some(replay) = idempotency::check_before(pool.get_ref(), &ctx, &req)? {
        return Ok(replay);
    }
    let mut conn = pool.get()?;
    let (now, off) = now_utc_naive();
    let before = serialize_item(&existing);
    diesel::update(lost_found_items::table.filter(lost_found_items::id.eq(id)))
        .set((
            lost_found_items::status.eq(STATUS_DELETED),
            lost_found_items::updated_at.eq(now),
            lost_found_items::updated_offset_minutes.eq(off),
        ))
        .execute(&mut conn)?;
    let updated = load_item(pool.get_ref(), id).await?;
    audit_svc::write(
        pool.get_ref(),
        audit_svc::AuditEntry {
            actor_user_id: Some(ctx.user.id),
            facility_id: Some(updated.facility_id),
            entity_type: PARENT_TYPE.into(),
            entity_id: updated.id,
            action: "soft_delete".into(),
            before_state: Some(before),
            after_state: Some(serialize_item(&updated)),
            request_id: ctx.request_id.clone(),
        },
    )?;
    let response = json!({ "status": "deleted", "id": id });
    idempotency::record_after(
        pool.get_ref(),
        &ctx,
        "DELETE",
        &format!("/api/lost-found/items/{}", id),
        200,
        &response,
    )?;
    Ok(HttpResponse::Ok().json(response))
}

async fn transition(
    pool: &DbPool,
    ctx: &RequestContext,
    existing: &LostFoundItem,
    to: &str,
    bounce_reason: Option<String>,
    action: &str,
) -> AppResult<()> {
    let mut conn = pool.get()?;
    let (now, off) = now_utc_naive();
    let before = serialize_item(existing);
    let reason_val = bounce_reason.clone();
    diesel::update(lost_found_items::table.filter(lost_found_items::id.eq(existing.id)))
        .set((
            lost_found_items::status.eq(to),
            lost_found_items::bounce_reason.eq(reason_val),
            lost_found_items::updated_at.eq(now),
            lost_found_items::updated_offset_minutes.eq(off),
        ))
        .execute(&mut conn)?;
    let after = load_item(pool, existing.id).await?;
    audit_svc::write(
        pool,
        audit_svc::AuditEntry {
            actor_user_id: Some(ctx.user.id),
            facility_id: Some(existing.facility_id),
            entity_type: PARENT_TYPE.into(),
            entity_id: existing.id,
            action: action.into(),
            before_state: Some(before),
            after_state: Some(serialize_item(&after)),
            request_id: ctx.request_id.clone(),
        },
    )?;
    Ok(())
}

async fn item_history(
    req: HttpRequest,
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
) -> AppResult<HttpResponse> {
    let ctx = require_ctx(&req)?;
    if !ctx.has_any_permission(&["lost_found.read", "lost_found.edit_draft", "lost_found.review"]) {
        return Err(AppError::Forbidden);
    }
    let id = path.into_inner();
    let existing = load_item(pool.get_ref(), id).await?;
    enforce_facility_scope(&ctx, existing.facility_id)?;
    let mut conn = pool.get()?;
    let rows: Vec<crate::models::audit::AuditLog> = audit_logs::table
        .filter(audit_logs::entity_type.eq(PARENT_TYPE))
        .filter(audit_logs::entity_id.eq(id))
        .order(audit_logs::created_at.asc())
        .load(&mut conn)?;
    let out: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "id": r.id,
                "action": r.action,
                "actorUserId": r.actor_user_id,
                "before": r.before_state,
                "after": r.after_state,
                "createdAt": r.created_at.format("%Y-%m-%dT%H:%M:%S").to_string(),
            })
        })
        .collect();
    Ok(HttpResponse::Ok().json(json!({ "history": out, "count": out.len() })))
}

#[derive(Debug, Deserialize)]
struct AttachmentBody {
    filename: String,
    #[serde(rename = "contentType")]
    content_type: String,
    #[serde(rename = "dataBase64")]
    data_base64: String,
}

async fn upload_attachment(
    req: HttpRequest,
    pool: web::Data<DbPool>,
    cfg: web::Data<Config>,
    path: web::Path<Uuid>,
    body: web::Json<AttachmentBody>,
) -> AppResult<HttpResponse> {
    let ctx = require_ctx(&req)?;
    if !ctx.has_any_permission(&["lost_found.edit_draft", "lost_found.review"]) {
        return Err(AppError::Forbidden);
    }
    let id = path.into_inner();
    let item = load_item(pool.get_ref(), id).await?;
    enforce_facility_scope(&ctx, item.facility_id)?;

    let raw = base64::engine::general_purpose::STANDARD
        .decode(body.data_base64.as_bytes())
        .map_err(|_| AppError::InvalidAttachment("dataBase64 is invalid".into()))?;

    if let Some(replay) = idempotency::check_before(pool.get_ref(), &ctx, &req)? {
        return Ok(replay);
    }

    let result = att_svc::upload(
        pool.get_ref(),
        &cfg.blob_dir,
        att_svc::UploadRequest {
            facility_id: item.facility_id,
            parent_type: PARENT_TYPE.to_string(),
            parent_id: item.id,
            filename: body.filename.clone(),
            mime_type: body.content_type.clone(),
            raw_bytes: raw,
            created_by: ctx.user.id,
        },
    )?;

    audit_svc::write(
        pool.get_ref(),
        audit_svc::AuditEntry {
            actor_user_id: Some(ctx.user.id),
            facility_id: Some(item.facility_id),
            entity_type: "attachment".into(),
            entity_id: result.attachment.id,
            action: "upload".into(),
            before_state: None,
            after_state: Some(json!({
                "id": result.attachment.id,
                "parentId": result.attachment.parent_id,
                "sha256": result.attachment.sha256,
                "sizeBytes": result.attachment.size_bytes,
                "deduplicated": result.deduplicated,
            })),
            request_id: ctx.request_id.clone(),
        },
    )?;

    // Supplement trigger: if the parent item has already been sent for review
    // or published, adding an attachment is a supplemental change that
    // reviewers and the original reporter should hear about. The prompt
    // explicitly lists `submission/supplement/review/change` triggers — this
    // is where supplement gets fired.
    if item.status == STATUS_IN_REVIEW || item.status == STATUS_PUBLISHED {
        let _ = notify::enqueue(
            pool.get_ref(),
            notify::Trigger {
                user_id: item.created_by,
                event_kind: notify::EVENT_SUPPLEMENT,
                template_code: "lost_found.supplemented".into(),
                facility_id: Some(item.facility_id),
                channel: notify::CHANNEL_IN_APP,
                to_address: None,
                fallback_subject: format!("New attachment on: {}", item.title),
                fallback_body: format!(
                    "An attachment ({}) was added to {}.",
                    result.attachment.filename, item.title
                ),
                payload: json!({
                    "item": { "id": item.id, "title": item.title, "status": item.status },
                    "attachment": {
                        "id": result.attachment.id,
                        "filename": result.attachment.filename,
                        "sha256": result.attachment.sha256,
                    }
                }),
            },
        );
    }

    let response = json!({
        "id": result.attachment.id,
        "parentId": result.attachment.parent_id,
        "parentType": result.attachment.parent_type,
        "facilityId": result.attachment.facility_id,
        "filename": result.attachment.filename,
        "contentType": result.attachment.mime_type,
        "sizeBytes": result.attachment.size_bytes,
        "sha256": result.attachment.sha256,
        "deduplicated": result.deduplicated,
        "createdAt": result.attachment.created_at.format("%Y-%m-%dT%H:%M:%S").to_string(),
    });
    idempotency::record_after(
        pool.get_ref(),
        &ctx,
        "POST",
        &format!("/api/lost-found/items/{}/attachments", id),
        201,
        &response,
    )?;
    Ok(HttpResponse::Created().json(response))
}

async fn list_attachments(
    req: HttpRequest,
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
) -> AppResult<HttpResponse> {
    let ctx = require_ctx(&req)?;
    if !ctx.has_any_permission(&["lost_found.read", "lost_found.edit_draft", "lost_found.review"]) {
        return Err(AppError::Forbidden);
    }
    let id = path.into_inner();
    let item = load_item(pool.get_ref(), id).await?;
    enforce_facility_scope(&ctx, item.facility_id)?;
    let rows = att_svc::list(pool.get_ref(), PARENT_TYPE, id)?;
    let out: Vec<Value> = rows
        .iter()
        .map(|a| {
            json!({
                "id": a.id,
                "filename": a.filename,
                "contentType": a.mime_type,
                "sizeBytes": a.size_bytes,
                "sha256": a.sha256,
                "createdAt": a.created_at.format("%Y-%m-%dT%H:%M:%S").to_string(),
            })
        })
        .collect();
    Ok(HttpResponse::Ok().json(json!({ "attachments": out, "count": out.len() })))
}

async fn delete_attachment(
    req: HttpRequest,
    pool: web::Data<DbPool>,
    path: web::Path<(Uuid, Uuid)>,
) -> AppResult<HttpResponse> {
    let ctx = require_ctx(&req)?;
    if !ctx.has_any_permission(&["lost_found.edit_draft", "lost_found.review"]) {
        return Err(AppError::Forbidden);
    }
    let (item_id, att_id) = path.into_inner();
    let item = load_item(pool.get_ref(), item_id).await?;
    enforce_facility_scope(&ctx, item.facility_id)?;
    if let Some(replay) = idempotency::check_before(pool.get_ref(), &ctx, &req)? {
        return Ok(replay);
    }
    att_svc::delete(pool.get_ref(), att_id, item.facility_id, PARENT_TYPE, item.id)?;
    let response = json!({ "status": "deleted", "id": att_id });
    idempotency::record_after(
        pool.get_ref(),
        &ctx,
        "DELETE",
        &format!("/api/lost-found/items/{}/attachments/{}", item_id, att_id),
        200,
        &response,
    )?;
    Ok(HttpResponse::Ok().json(response))
}
