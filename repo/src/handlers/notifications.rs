use actix_web::dev::HttpServiceFactory;
use actix_web::{web, HttpMessage, HttpRequest, HttpResponse};
use chrono::Duration;
use diesel::prelude::*;
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::config::Config;
use crate::db::DbPool;
use crate::errors::{AppError, AppResult};
use crate::middleware::idempotency;
use crate::middleware::request_context::RequestContext;
use crate::models::notification::{
    NewNotificationSubscription, NewNotificationTemplate, Notification, NotificationSubscription,
    NotificationTemplate, OutboxDelivery,
};
use crate::schema::{
    notification_subscriptions, notification_templates, notifications, outbox_deliveries,
};
use crate::services::audit as audit_svc;
use crate::services::notify;
use crate::services::time::now_utc_naive;

pub fn scope() -> impl HttpServiceFactory {
    web::scope("/notifications")
        .wrap(crate::middleware::auth::Authenticate)
        .route("/inbox", web::get().to(list_inbox))
        .route("/inbox/{id}/read", web::post().to(mark_read))
        .route("/inbox/mark-all-read", web::post().to(mark_all_read))
        .service(
            web::resource("/templates")
                .route(web::get().to(list_templates))
                .route(web::post().to(create_template)),
        )
        .service(
            web::resource("/templates/{id}")
                .route(web::put().to(update_template))
                .route(web::delete().to(delete_template)),
        )
        .route("/outbox", web::get().to(list_outbox))
        .route("/outbox/export", web::get().to(outbox_export))
        .route("/outbox/import-results", web::post().to(import_results))
        .route("/dispatch", web::post().to(dispatch))
        .service(
            web::resource("/subscriptions")
                .route(web::get().to(list_subscriptions))
                .route(web::put().to(put_subscriptions)),
        )
}

fn require_ctx(req: &HttpRequest) -> AppResult<RequestContext> {
    let ext = req.extensions();
    ext.get::<RequestContext>()
        .cloned()
        .ok_or(AppError::Unauthenticated)
}

fn serialize_notification(n: &Notification) -> Value {
    json!({
        "id": n.id,
        "userId": n.user_id,
        "eventKind": n.event_kind,
        "subject": n.subject,
        "body": n.body,
        "payload": n.payload,
        "isRead": n.is_read,
        "readAt": n.read_at.as_ref().map(|t| t.format("%Y-%m-%dT%H:%M:%S").to_string()),
        "readOffsetMinutes": n.read_offset_minutes,
        "createdAt": n.created_at.format("%Y-%m-%dT%H:%M:%S").to_string(),
        "createdOffsetMinutes": n.created_offset_minutes,
    })
}

fn serialize_template(t: &NotificationTemplate) -> Value {
    json!({
        "id": t.id,
        "code": t.code,
        "subject": t.subject,
        "body": t.body,
        "isActive": t.is_active,
        "createdAt": t.created_at.format("%Y-%m-%dT%H:%M:%S").to_string(),
        "updatedAt": t.updated_at.format("%Y-%m-%dT%H:%M:%S").to_string(),
    })
}

fn serialize_outbox(o: &OutboxDelivery) -> Value {
    json!({
        "id": o.id,
        "userId": o.user_id,
        "facilityId": o.facility_id,
        "eventKind": o.event_kind,
        "templateCode": o.template_code,
        "channel": o.channel,
        "toAddress": o.to_address,
        "subject": o.subject,
        "body": o.body,
        "payload": o.payload,
        "status": o.status,
        "attemptCount": o.attempt_count,
        "nextAttemptAt": o.next_attempt_at.as_ref().map(|t| t.format("%Y-%m-%dT%H:%M:%S").to_string()),
        "nextAttemptOffsetMinutes": o.next_attempt_offset_minutes,
        "lastError": o.last_error,
        "createdAt": o.created_at.format("%Y-%m-%dT%H:%M:%S").to_string(),
        "createdOffsetMinutes": o.created_offset_minutes,
        "updatedAt": o.updated_at.format("%Y-%m-%dT%H:%M:%S").to_string(),
        "updatedOffsetMinutes": o.updated_offset_minutes,
    })
}

/// Any authenticated user gets to read *their own* inbox/subscriptions, but
/// only when they hold `notifications.read` (or the admin variant). This keeps
/// permission-to-resource mapping explicit even for self-scoped reads.
fn require_notifications_reader(ctx: &RequestContext) -> AppResult<()> {
    if ctx.has_any_permission(&["notifications.read", "notifications.admin"]) {
        Ok(())
    } else {
        Err(AppError::Forbidden)
    }
}

async fn list_inbox(req: HttpRequest, pool: web::Data<DbPool>) -> AppResult<HttpResponse> {
    let ctx = require_ctx(&req)?;
    require_notifications_reader(&ctx)?;
    let mut conn = pool.get()?;
    let rows: Vec<Notification> = notifications::table
        .filter(notifications::user_id.eq(ctx.user.id))
        .order(notifications::created_at.desc())
        .limit(500)
        .load(&mut conn)?;
    let out: Vec<Value> = rows.iter().map(serialize_notification).collect();
    Ok(HttpResponse::Ok().json(json!({ "inbox": out, "count": out.len() })))
}

async fn mark_read(
    req: HttpRequest,
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
) -> AppResult<HttpResponse> {
    let ctx = require_ctx(&req)?;
    require_notifications_reader(&ctx)?;
    let id = path.into_inner();
    if let Some(replay) = idempotency::check_before(pool.get_ref(), &ctx, &req)? {
        return Ok(replay);
    }
    let mut conn = pool.get()?;
    let (now, off) = now_utc_naive();
    let affected = diesel::update(
        notifications::table
            .filter(notifications::id.eq(id))
            .filter(notifications::user_id.eq(ctx.user.id)),
    )
    .set((
        notifications::is_read.eq(true),
        notifications::read_at.eq(Some(now)),
        notifications::read_offset_minutes.eq(Some(off)),
    ))
    .execute(&mut conn)?;
    if affected == 0 {
        return Err(AppError::NotFound);
    }
    let response = json!({ "status": "read", "id": id });
    idempotency::record_after(
        pool.get_ref(),
        &ctx,
        "POST",
        &format!("/api/notifications/inbox/{}/read", id),
        200,
        &response,
    )?;
    Ok(HttpResponse::Ok().json(response))
}

async fn mark_all_read(req: HttpRequest, pool: web::Data<DbPool>) -> AppResult<HttpResponse> {
    let ctx = require_ctx(&req)?;
    require_notifications_reader(&ctx)?;
    if let Some(replay) = idempotency::check_before(pool.get_ref(), &ctx, &req)? {
        return Ok(replay);
    }
    let mut conn = pool.get()?;
    let (now, off) = now_utc_naive();
    let affected = diesel::update(
        notifications::table
            .filter(notifications::user_id.eq(ctx.user.id))
            .filter(notifications::is_read.eq(false)),
    )
    .set((
        notifications::is_read.eq(true),
        notifications::read_at.eq(Some(now)),
        notifications::read_offset_minutes.eq(Some(off)),
    ))
    .execute(&mut conn)?;
    let response = json!({ "status": "marked", "updated": affected });
    idempotency::record_after(
        pool.get_ref(),
        &ctx,
        "POST",
        "/api/notifications/inbox/mark-all-read",
        200,
        &response,
    )?;
    Ok(HttpResponse::Ok().json(response))
}

async fn list_templates(req: HttpRequest, pool: web::Data<DbPool>) -> AppResult<HttpResponse> {
    let ctx = require_ctx(&req)?;
    // Templates contain messaging policy — only notification admins may read them.
    if !ctx.has_any_permission(&["notifications.admin"]) {
        return Err(AppError::Forbidden);
    }
    let mut conn = pool.get()?;
    let rows: Vec<NotificationTemplate> = notification_templates::table
        .order(notification_templates::code.asc())
        .load(&mut conn)?;
    let out: Vec<Value> = rows.iter().map(serialize_template).collect();
    Ok(HttpResponse::Ok().json(json!({ "templates": out, "count": out.len() })))
}

#[derive(Debug, Deserialize)]
struct TemplateBody {
    code: String,
    subject: String,
    body: String,
    #[serde(default = "default_true", rename = "isActive")]
    is_active: bool,
}

fn default_true() -> bool {
    true
}

fn validate_template_variables(subject: &str, body: &str) -> AppResult<()> {
    let re = notify::variable_regex();
    for hay in [subject, body] {
        for cap in re.captures_iter(hay) {
            let var = cap.get(1).map(|m| m.as_str()).unwrap_or("");
            if !notify::ALLOWED_VARIABLES.contains(&var) {
                return Err(AppError::Validation {
                    message: format!("template references disallowed variable: {}", var),
                    details: json!({ "field": "body", "variable": var }),
                });
            }
        }
    }
    Ok(())
}

async fn create_template(
    req: HttpRequest,
    pool: web::Data<DbPool>,
    body: web::Json<TemplateBody>,
) -> AppResult<HttpResponse> {
    let ctx = require_ctx(&req)?;
    if !ctx.has_any_permission(&["notifications.admin"]) {
        return Err(AppError::Forbidden);
    }
    validate_template_variables(&body.subject, &body.body)?;
    if let Some(replay) = idempotency::check_before(pool.get_ref(), &ctx, &req)? {
        return Ok(replay);
    }
    let mut conn = pool.get()?;
    let (now, off) = now_utc_naive();
    let saved: NotificationTemplate = diesel::insert_into(notification_templates::table)
        .values(NewNotificationTemplate {
            id: Uuid::new_v4(),
            code: body.code.clone(),
            subject: body.subject.clone(),
            body: body.body.clone(),
            is_active: body.is_active,
            created_at: now,
            created_offset_minutes: off,
            updated_at: now,
            updated_offset_minutes: off,
        })
        .get_result(&mut conn)?;
    audit_svc::write(
        pool.get_ref(),
        audit_svc::AuditEntry {
            actor_user_id: Some(ctx.user.id),
            facility_id: None,
            entity_type: "notification_template".into(),
            entity_id: saved.id,
            action: "create".into(),
            before_state: None,
            after_state: Some(serialize_template(&saved)),
            request_id: ctx.request_id.clone(),
        },
    )?;
    let response = serialize_template(&saved);
    idempotency::record_after(
        pool.get_ref(),
        &ctx,
        "POST",
        "/api/notifications/templates",
        201,
        &response,
    )?;
    Ok(HttpResponse::Created().json(response))
}

async fn update_template(
    req: HttpRequest,
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
    body: web::Json<TemplateBody>,
) -> AppResult<HttpResponse> {
    let ctx = require_ctx(&req)?;
    if !ctx.has_any_permission(&["notifications.admin"]) {
        return Err(AppError::Forbidden);
    }
    validate_template_variables(&body.subject, &body.body)?;
    if let Some(replay) = idempotency::check_before(pool.get_ref(), &ctx, &req)? {
        return Ok(replay);
    }
    let id = path.into_inner();
    let mut conn = pool.get()?;
    let (now, off) = now_utc_naive();
    let affected = diesel::update(notification_templates::table.filter(notification_templates::id.eq(id)))
        .set((
            notification_templates::code.eq(&body.code),
            notification_templates::subject.eq(&body.subject),
            notification_templates::body.eq(&body.body),
            notification_templates::is_active.eq(body.is_active),
            notification_templates::updated_at.eq(now),
            notification_templates::updated_offset_minutes.eq(off),
        ))
        .execute(&mut conn)?;
    if affected == 0 {
        return Err(AppError::NotFound);
    }
    let updated: NotificationTemplate = notification_templates::table
        .filter(notification_templates::id.eq(id))
        .first(&mut conn)?;
    audit_svc::write(
        pool.get_ref(),
        audit_svc::AuditEntry {
            actor_user_id: Some(ctx.user.id),
            facility_id: None,
            entity_type: "notification_template".into(),
            entity_id: updated.id,
            action: "update".into(),
            before_state: None,
            after_state: Some(serialize_template(&updated)),
            request_id: ctx.request_id.clone(),
        },
    )?;
    let response = serialize_template(&updated);
    idempotency::record_after(
        pool.get_ref(),
        &ctx,
        "PUT",
        &format!("/api/notifications/templates/{}", id),
        200,
        &response,
    )?;
    Ok(HttpResponse::Ok().json(response))
}

async fn delete_template(
    req: HttpRequest,
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
) -> AppResult<HttpResponse> {
    let ctx = require_ctx(&req)?;
    if !ctx.has_any_permission(&["notifications.admin"]) {
        return Err(AppError::Forbidden);
    }
    if let Some(replay) = idempotency::check_before(pool.get_ref(), &ctx, &req)? {
        return Ok(replay);
    }
    let id = path.into_inner();
    let mut conn = pool.get()?;
    let affected =
        diesel::delete(notification_templates::table.filter(notification_templates::id.eq(id)))
            .execute(&mut conn)?;
    if affected == 0 {
        return Err(AppError::NotFound);
    }
    audit_svc::write(
        pool.get_ref(),
        audit_svc::AuditEntry {
            actor_user_id: Some(ctx.user.id),
            facility_id: None,
            entity_type: "notification_template".into(),
            entity_id: id,
            action: "delete".into(),
            before_state: None,
            after_state: None,
            request_id: ctx.request_id.clone(),
        },
    )?;
    let response = json!({ "status": "deleted", "id": id });
    idempotency::record_after(
        pool.get_ref(),
        &ctx,
        "DELETE",
        &format!("/api/notifications/templates/{}", id),
        200,
        &response,
    )?;
    Ok(HttpResponse::Ok().json(response))
}

#[derive(Debug, Deserialize)]
struct OutboxQuery {
    status: Option<String>,
    channel: Option<String>,
    #[serde(rename = "facilityId")]
    facility_id: Option<Uuid>,
}

async fn list_outbox(
    req: HttpRequest,
    pool: web::Data<DbPool>,
    q: web::Query<OutboxQuery>,
) -> AppResult<HttpResponse> {
    let ctx = require_ctx(&req)?;
    if !ctx.has_any_permission(&["notifications.admin"]) {
        return Err(AppError::Forbidden);
    }
    let mut conn = pool.get()?;
    let mut query = outbox_deliveries::table.into_boxed();
    if let Some(set) = ctx.allowed_facilities() {
        let ids: Vec<Uuid> = set.into_iter().collect();
        query = query.filter(outbox_deliveries::facility_id.eq_any(ids));
    }
    if let Some(s) = &q.status {
        query = query.filter(outbox_deliveries::status.eq(s));
    }
    if let Some(c) = &q.channel {
        query = query.filter(outbox_deliveries::channel.eq(c));
    }
    if let Some(fid) = q.facility_id {
        if let Some(set) = ctx.allowed_facilities() {
            if !set.contains(&fid) {
                return Err(AppError::OutOfScope);
            }
        }
        query = query.filter(outbox_deliveries::facility_id.eq(fid));
    }
    let rows: Vec<OutboxDelivery> = query
        .order(outbox_deliveries::created_at.desc())
        .limit(500)
        .load(&mut conn)?;
    let out: Vec<Value> = rows.iter().map(serialize_outbox).collect();
    Ok(HttpResponse::Ok().json(json!({ "outbox": out, "count": out.len() })))
}

async fn outbox_export(
    req: HttpRequest,
    pool: web::Data<DbPool>,
    cfg: web::Data<Config>,
) -> AppResult<HttpResponse> {
    let ctx = require_ctx(&req)?;
    if !ctx.has_any_permission(&["notifications.admin"]) {
        return Err(AppError::Forbidden);
    }
    let mut conn = pool.get()?;
    let mut query = outbox_deliveries::table.into_boxed();
    if let Some(set) = ctx.allowed_facilities() {
        let ids: Vec<Uuid> = set.into_iter().collect();
        query = query.filter(outbox_deliveries::facility_id.eq_any(ids));
    }
    let rows: Vec<OutboxDelivery> = query
        .filter(outbox_deliveries::status.eq("PENDING"))
        .order(outbox_deliveries::created_at.asc())
        .load(&mut conn)?;
    let mut body = String::new();
    for r in &rows {
        body.push_str(&serde_json::to_string(&serialize_outbox(r)).unwrap());
        body.push('\n');
    }

    // Persist a timestamped NDJSON snapshot under OUTBOX_EXPORT_DIR so the
    // offline relay has a durable file record of what was exported, not just
    // the streaming response. Failures here are logged but do not abort the
    // HTTP response — the stream is still the source of truth.
    if !body.is_empty() {
        if let Err(e) = write_export_snapshot(&cfg.outbox_export_dir, &body) {
            tracing::warn!(error = %e, "outbox_export snapshot failed");
        }
    }

    Ok(HttpResponse::Ok()
        .content_type("application/x-ndjson")
        .body(body))
}

/// Write NDJSON to `<dir>/outbox-<UTC RFC3339-ish>-<uuid>.ndjson`. The uuid
/// suffix keeps concurrent exports from overwriting each other.
fn write_export_snapshot(dir: &str, body: &str) -> std::io::Result<std::path::PathBuf> {
    use std::fs;
    use std::io::Write;
    fs::create_dir_all(dir)?;
    let (now, _) = now_utc_naive();
    let name = format!(
        "outbox-{}-{}.ndjson",
        now.format("%Y%m%dT%H%M%S"),
        Uuid::new_v4().simple()
    );
    let path = std::path::PathBuf::from(dir).join(name);
    let mut f = fs::File::create(&path)?;
    f.write_all(body.as_bytes())?;
    Ok(path)
}

#[derive(Debug, Deserialize)]
struct ImportResult {
    id: Uuid,
    status: String,
    #[serde(default)]
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ImportBody {
    results: Vec<ImportResult>,
}

pub const BACKOFF_SCHEDULE_MIN: &[i64] = &[1, 5, 30];
pub const MAX_ATTEMPTS: i32 = 4;

async fn import_results(
    req: HttpRequest,
    pool: web::Data<DbPool>,
    body: web::Json<ImportBody>,
) -> AppResult<HttpResponse> {
    let ctx = require_ctx(&req)?;
    if !ctx.has_any_permission(&["notifications.admin"]) {
        return Err(AppError::Forbidden);
    }
    if let Some(replay) = idempotency::check_before(pool.get_ref(), &ctx, &req)? {
        return Ok(replay);
    }
    let allowed = ctx.allowed_facilities();
    let mut conn = pool.get()?;
    let (now, off) = now_utc_naive();
    let mut acked = 0;
    let mut failed = 0;
    let mut dead = 0;
    for r in &body.results {
        let existing: Option<OutboxDelivery> = outbox_deliveries::table
            .filter(outbox_deliveries::id.eq(r.id))
            .first(&mut conn)
            .optional()?;
        let Some(existing) = existing else {
            continue;
        };
        // Facility-scoped admins cannot ack rows outside their scope.
        if let Some(set) = &allowed {
            match existing.facility_id {
                Some(fid) if set.contains(&fid) => {}
                _ => continue,
            }
        }
        match r.status.as_str() {
            "SENT" | "sent" | "ok" => {
                diesel::update(outbox_deliveries::table.filter(outbox_deliveries::id.eq(r.id)))
                    .set((
                        outbox_deliveries::status.eq("SENT"),
                        outbox_deliveries::updated_at.eq(now),
                        outbox_deliveries::updated_offset_minutes.eq(off),
                        outbox_deliveries::last_error.eq::<Option<String>>(None),
                        outbox_deliveries::next_attempt_at.eq::<Option<chrono::NaiveDateTime>>(None),
                        outbox_deliveries::next_attempt_offset_minutes.eq::<Option<i16>>(None),
                    ))
                    .execute(&mut conn)?;
                acked += 1;
            }
            _ => {
                let new_attempt = existing.attempt_count + 1;
                let (new_status, next_attempt, next_off) = if new_attempt >= MAX_ATTEMPTS {
                    dead += 1;
                    ("DEAD", None, None)
                } else {
                    failed += 1;
                    let idx = (new_attempt - 1).clamp(0, (BACKOFF_SCHEDULE_MIN.len() - 1) as i32) as usize;
                    let delay = BACKOFF_SCHEDULE_MIN[idx];
                    ("PENDING", Some(now + Duration::minutes(delay)), Some(off))
                };
                diesel::update(outbox_deliveries::table.filter(outbox_deliveries::id.eq(r.id)))
                    .set((
                        outbox_deliveries::status.eq(new_status),
                        outbox_deliveries::attempt_count.eq(new_attempt),
                        outbox_deliveries::next_attempt_at.eq(next_attempt),
                        outbox_deliveries::next_attempt_offset_minutes.eq(next_off),
                        outbox_deliveries::last_error.eq(r.error.clone()),
                        outbox_deliveries::updated_at.eq(now),
                        outbox_deliveries::updated_offset_minutes.eq(off),
                    ))
                    .execute(&mut conn)?;
            }
        }
    }
    let response = json!({
        "acked": acked,
        "failed": failed,
        "dead": dead,
    });
    idempotency::record_after(
        pool.get_ref(),
        &ctx,
        "POST",
        "/api/notifications/outbox/import-results",
        200,
        &response,
    )?;
    Ok(HttpResponse::Ok().json(response))
}

async fn list_subscriptions(req: HttpRequest, pool: web::Data<DbPool>) -> AppResult<HttpResponse> {
    let ctx = require_ctx(&req)?;
    require_notifications_reader(&ctx)?;
    let mut conn = pool.get()?;
    let rows: Vec<NotificationSubscription> = notification_subscriptions::table
        .filter(notification_subscriptions::user_id.eq(ctx.user.id))
        .load(&mut conn)?;
    let out: Vec<Value> = rows
        .iter()
        .map(|s| {
            json!({
                "eventKind": s.event_kind,
                "enabled": s.enabled,
            })
        })
        .collect();
    Ok(HttpResponse::Ok().json(json!({ "subscriptions": out, "count": out.len() })))
}

#[derive(Debug, Deserialize)]
struct SubPutBody {
    subscriptions: Vec<SubPutItem>,
}

#[derive(Debug, Deserialize)]
struct SubPutItem {
    #[serde(rename = "eventKind")]
    event_kind: String,
    enabled: bool,
}

async fn put_subscriptions(
    req: HttpRequest,
    pool: web::Data<DbPool>,
    body: web::Json<SubPutBody>,
) -> AppResult<HttpResponse> {
    let ctx = require_ctx(&req)?;
    require_notifications_reader(&ctx)?;
    if let Some(replay) = idempotency::check_before(pool.get_ref(), &ctx, &req)? {
        return Ok(replay);
    }
    let mut conn = pool.get()?;
    let (now, off) = now_utc_naive();
    for item in &body.subscriptions {
        diesel::insert_into(notification_subscriptions::table)
            .values(NewNotificationSubscription {
                user_id: ctx.user.id,
                event_kind: item.event_kind.clone(),
                enabled: item.enabled,
                updated_at: now,
                updated_offset_minutes: off,
            })
            .on_conflict((
                notification_subscriptions::user_id,
                notification_subscriptions::event_kind,
            ))
            .do_update()
            .set((
                notification_subscriptions::enabled.eq(item.enabled),
                notification_subscriptions::updated_at.eq(now),
                notification_subscriptions::updated_offset_minutes.eq(off),
            ))
            .execute(&mut conn)?;
    }
    let rows: Vec<NotificationSubscription> = notification_subscriptions::table
        .filter(notification_subscriptions::user_id.eq(ctx.user.id))
        .load(&mut conn)?;
    let out: Vec<Value> = rows
        .iter()
        .map(|s| json!({ "eventKind": s.event_kind, "enabled": s.enabled }))
        .collect();
    let response = json!({ "subscriptions": out, "count": out.len() });
    idempotency::record_after(
        pool.get_ref(),
        &ctx,
        "PUT",
        "/api/notifications/subscriptions",
        200,
        &response,
    )?;
    Ok(HttpResponse::Ok().json(response))
}

#[derive(Debug, Deserialize)]
struct DispatchBody {
    #[serde(rename = "userId")]
    user_id: Uuid,
    #[serde(rename = "eventKind")]
    event_kind: String,
    #[serde(rename = "templateCode")]
    template_code: String,
    channel: String,
    #[serde(default, rename = "toAddress")]
    to_address: Option<String>,
    #[serde(default, rename = "facilityId")]
    facility_id: Option<Uuid>,
    #[serde(default)]
    subject: Option<String>,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    payload: Value,
}

/// Static leaf of Trigger.channel — we need a `&'static str` for the shared
/// `Trigger` type. Resolve the caller's channel string to one of the four
/// supported constants at dispatch time.
fn channel_static(s: &str) -> AppResult<&'static str> {
    match s {
        notify::CHANNEL_IN_APP => Ok(notify::CHANNEL_IN_APP),
        notify::CHANNEL_EMAIL => Ok(notify::CHANNEL_EMAIL),
        notify::CHANNEL_SMS => Ok(notify::CHANNEL_SMS),
        notify::CHANNEL_WEBHOOK => Ok(notify::CHANNEL_WEBHOOK),
        other => Err(AppError::Validation {
            message: format!("unsupported channel: {}", other),
            details: json!({ "field": "channel", "allowed": notify::ALL_CHANNELS }),
        }),
    }
}

fn event_kind_static(s: &str) -> AppResult<&'static str> {
    match s {
        notify::EVENT_SUBMISSION => Ok(notify::EVENT_SUBMISSION),
        notify::EVENT_SUPPLEMENT => Ok(notify::EVENT_SUPPLEMENT),
        notify::EVENT_REVIEW => Ok(notify::EVENT_REVIEW),
        notify::EVENT_CHANGE => Ok(notify::EVENT_CHANGE),
        other => Err(AppError::Validation {
            message: format!("unsupported eventKind: {}", other),
            details: json!({
                "field": "eventKind",
                "allowed": [
                    notify::EVENT_SUBMISSION,
                    notify::EVENT_SUPPLEMENT,
                    notify::EVENT_REVIEW,
                    notify::EVENT_CHANGE,
                ]
            }),
        }),
    }
}

fn validate_destination(channel: &str, to_address: &Option<String>) -> AppResult<()> {
    // In-app rows do not need a destination (they are persisted to the user's
    // inbox by user_id). Every other channel is a real outbound transport that
    // must have a destination — otherwise the outbox export would emit a row
    // no external relay could act on.
    if channel == notify::CHANNEL_IN_APP {
        return Ok(());
    }
    match to_address.as_deref() {
        Some(addr) if !addr.trim().is_empty() => Ok(()),
        _ => Err(AppError::Validation {
            message: format!("toAddress required for channel={}", channel),
            details: json!({ "field": "toAddress", "channel": channel }),
        }),
    }
}

/// POST /api/notifications/dispatch
///
/// Admin-only fan-out path for email/SMS/webhook (and optionally in-app)
/// outbox rows. A template is still rendered server-side so the payload can't
/// smuggle arbitrary variables, and facility scope is enforced against the
/// caller's allowed facilities.
async fn dispatch(
    req: HttpRequest,
    pool: web::Data<DbPool>,
    body: web::Json<DispatchBody>,
) -> AppResult<HttpResponse> {
    let ctx = require_ctx(&req)?;
    if !ctx.has_any_permission(&["notifications.admin"]) {
        return Err(AppError::Forbidden);
    }
    let channel = channel_static(&body.channel)?;
    let event_kind = event_kind_static(&body.event_kind)?;
    validate_destination(channel, &body.to_address)?;

    // Enforce facility scope: scoped admins can only dispatch for facilities
    // they control. A None facility_id is reserved for system-wide messages
    // and requires wildcard scope.
    if let Some(set) = ctx.allowed_facilities() {
        match body.facility_id {
            Some(fid) if set.contains(&fid) => {}
            _ => return Err(AppError::OutOfScope),
        }
    }

    if let Some(replay) = idempotency::check_before(pool.get_ref(), &ctx, &req)? {
        return Ok(replay);
    }

    let fallback_subject = body
        .subject
        .clone()
        .unwrap_or_else(|| format!("{} notification", event_kind));
    let fallback_body = body.body.clone().unwrap_or_default();

    let enqueued = notify::enqueue(
        pool.get_ref(),
        notify::Trigger {
            user_id: body.user_id,
            event_kind,
            template_code: body.template_code.clone(),
            facility_id: body.facility_id,
            channel,
            to_address: body.to_address.clone(),
            fallback_subject,
            fallback_body,
            payload: body.payload.clone(),
        },
    )?;

    audit_svc::write(
        pool.get_ref(),
        audit_svc::AuditEntry {
            actor_user_id: Some(ctx.user.id),
            facility_id: body.facility_id,
            entity_type: "outbox_delivery".into(),
            entity_id: body.user_id,
            action: "dispatch".into(),
            before_state: None,
            after_state: Some(json!({
                "channel": channel,
                "eventKind": event_kind,
                "templateCode": body.template_code,
                "toAddress": body.to_address,
                "enqueued": enqueued,
            })),
            request_id: ctx.request_id.clone(),
        },
    )?;

    let response = json!({
        "status": if enqueued { "enqueued" } else { "skipped_by_subscription" },
        "channel": channel,
        "templateCode": body.template_code,
    });
    idempotency::record_after(
        pool.get_ref(),
        &ctx,
        "POST",
        "/api/notifications/dispatch",
        200,
        &response,
    )?;
    Ok(HttpResponse::Ok().json(response))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allowed_variable_passes() {
        assert!(validate_template_variables("Hi {{ item.title }}", "Body {{ actor.displayName }}").is_ok());
    }

    #[test]
    fn disallowed_variable_rejected() {
        assert!(validate_template_variables("Hi", "{{ system.secret }}").is_err());
    }

    #[test]
    fn backoff_schedule_matches_spec() {
        assert_eq!(BACKOFF_SCHEDULE_MIN, &[1, 5, 30]);
        assert_eq!(MAX_ATTEMPTS, 4);
    }
}
