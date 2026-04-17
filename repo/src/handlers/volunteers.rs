use actix_web::dev::HttpServiceFactory;
use actix_web::{web, HttpMessage, HttpRequest, HttpResponse};
use chrono::{Duration, NaiveDate};
use diesel::prelude::*;
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::db::DbPool;
use crate::errors::{AppError, AppResult};
use crate::middleware::idempotency;
use crate::middleware::request_context::RequestContext;
use crate::models::volunteer::{NewQualification, NewVolunteer, Qualification, Volunteer};
use crate::schema::{qualifications, stores, volunteers};
use crate::services::audit as audit_svc;
use crate::services::crypto;
use crate::services::notify;
use crate::services::time::now_utc_naive;

pub fn scope() -> impl HttpServiceFactory {
    web::scope("/volunteers")
        .wrap(crate::middleware::auth::Authenticate)
        .service(
            web::resource("")
                .route(web::post().to(create_volunteer))
                .route(web::get().to(list_volunteers)),
        )
        .service(
            web::resource("/{id}")
                .route(web::get().to(get_volunteer))
                .route(web::put().to(update_volunteer))
                .route(web::delete().to(delete_volunteer)),
        )
        .service(
            web::resource("/{id}/qualifications")
                .route(web::get().to(list_qualifications))
                .route(web::post().to(create_qualification)),
        )
        .route(
            "/{id}/qualifications/{qualificationId}",
            web::delete().to(delete_qualification),
        )
}

fn require_ctx(req: &HttpRequest) -> AppResult<RequestContext> {
    let ext = req.extensions();
    ext.get::<RequestContext>()
        .cloned()
        .ok_or(AppError::Unauthenticated)
}

fn enforce_scope(ctx: &RequestContext, facility_id: Uuid) -> AppResult<()> {
    match ctx.allowed_facilities() {
        None => Ok(()),
        Some(set) if set.contains(&facility_id) => Ok(()),
        _ => Err(AppError::OutOfScope),
    }
}

/// Field-level write gate for sensitive volunteer/qualification data.
///
/// Only callers whose role has the field in its `field_allowlist` (or the
/// wildcard `*`) may write a non-empty value. A non-allowlisted caller that
/// supplies a sensitive field gets a 403 rather than silently stripping the
/// field, so clients learn immediately when a role can't set it.
fn enforce_field_write(
    ctx: &RequestContext,
    field: &str,
    value: Option<&str>,
) -> AppResult<()> {
    let has_value = matches!(value, Some(v) if !v.is_empty());
    if has_value && !ctx.can_view_field(field) {
        return Err(AppError::Forbidden);
    }
    Ok(())
}

/// Same as `enforce_field_write` but for `Option<Option<String>>` PATCH-style
/// fields: callers using `Some(None)` to clear a value still need the field
/// permission (otherwise a non-privileged role could wipe someone's gov_id).
fn enforce_field_patch(
    ctx: &RequestContext,
    field: &str,
    value: &Option<Option<String>>,
) -> AppResult<()> {
    match value {
        Some(inner) => {
            if !ctx.can_view_field(field) {
                return Err(AppError::Forbidden);
            }
            let _ = inner; // presence of the key implies intent to change
            Ok(())
        }
        None => Ok(()),
    }
}

fn serialize_volunteer(v: &Volunteer, ctx: &RequestContext) -> Value {
    let show_gov = ctx.can_view_field("gov_id");
    let show_notes = ctx.can_view_field("private_notes");

    let gov_id_val = match (&v.gov_id_encrypted, &v.gov_id_last4) {
        (Some(enc), Some(last4)) => {
            if show_gov {
                match crypto::decrypt(enc) {
                    Ok(bytes) => Value::String(String::from_utf8_lossy(&bytes).to_string()),
                    Err(_) => Value::String(crypto::mask_last4(last4)),
                }
            } else {
                Value::String(crypto::mask_last4(last4))
            }
        }
        _ => Value::Null,
    };

    let notes_val = match &v.private_notes_encrypted {
        Some(enc) => {
            if show_notes {
                match crypto::decrypt(enc) {
                    Ok(bytes) => Value::String(String::from_utf8_lossy(&bytes).to_string()),
                    Err(_) => Value::String("****".into()),
                }
            } else {
                Value::String("****".into())
            }
        }
        None => Value::Null,
    };

    json!({
        "id": v.id,
        "facilityId": v.facility_id,
        "fullName": v.full_name,
        "contactEmail": v.contact_email,
        "contactPhone": v.contact_phone,
        "govId": gov_id_val,
        "privateNotes": notes_val,
        "isActive": v.is_active,
        "createdAt": v.created_at.format("%Y-%m-%dT%H:%M:%S").to_string(),
        "updatedAt": v.updated_at.format("%Y-%m-%dT%H:%M:%S").to_string(),
    })
}

fn serialize_qual(q: &Qualification, ctx: &RequestContext) -> Value {
    let show_cert = ctx.can_view_field("certificate");
    let cert_val = match (&q.certificate_encrypted, &q.certificate_last4) {
        (Some(enc), Some(last4)) => {
            if show_cert {
                match crypto::decrypt(enc) {
                    Ok(bytes) => Value::String(String::from_utf8_lossy(&bytes).to_string()),
                    Err(_) => Value::String(crypto::mask_last4(last4)),
                }
            } else {
                Value::String(crypto::mask_last4(last4))
            }
        }
        _ => Value::Null,
    };
    json!({
        "id": q.id,
        "volunteerId": q.volunteer_id,
        "kind": q.kind,
        "issuer": q.issuer,
        "certificate": cert_val,
        "issuedOn": q.issued_on.format("%m/%d/%Y").to_string(),
        "expiresOn": q.expires_on.as_ref().map(|d| d.format("%m/%d/%Y").to_string()),
        "createdAt": q.created_at.format("%Y-%m-%dT%H:%M:%S").to_string(),
    })
}

#[derive(Debug, Deserialize)]
struct CreateBody {
    #[serde(rename = "facilityId")]
    facility_id: Uuid,
    #[serde(rename = "fullName")]
    full_name: String,
    #[serde(rename = "contactEmail")]
    contact_email: Option<String>,
    #[serde(rename = "contactPhone")]
    contact_phone: Option<String>,
    #[serde(rename = "govId")]
    gov_id: Option<String>,
    #[serde(rename = "privateNotes")]
    private_notes: Option<String>,
}

async fn create_volunteer(
    req: HttpRequest,
    pool: web::Data<DbPool>,
    body: web::Json<CreateBody>,
) -> AppResult<HttpResponse> {
    let ctx = require_ctx(&req)?;
    if !ctx.has_any_permission(&["volunteers.write"]) {
        return Err(AppError::Forbidden);
    }
    enforce_scope(&ctx, body.facility_id)?;
    if body.full_name.trim().is_empty() {
        return Err(AppError::Validation {
            message: "fullName required".into(),
            details: json!({ "field": "fullName" }),
        });
    }
    // Field-level write authorization: the sensitive fields are the same ones
    // the field_allowlist controls on reads. A caller without the allowlist
    // cannot create a row that sets them.
    enforce_field_write(&ctx, "gov_id", body.gov_id.as_deref())?;
    enforce_field_write(&ctx, "private_notes", body.private_notes.as_deref())?;
    if let Some(replay) = idempotency::check_before(pool.get_ref(), &ctx, &req)? {
        return Ok(replay);
    }
    let mut conn = pool.get()?;
    let fac_ok: Option<Uuid> = stores::table
        .filter(stores::id.eq(body.facility_id))
        .select(stores::id)
        .first(&mut conn)
        .optional()?;
    if fac_ok.is_none() {
        return Err(AppError::Validation {
            message: "facility not found".into(),
            details: json!({ "field": "facilityId" }),
        });
    }

    let (gov_enc, gov_last4) = match &body.gov_id {
        Some(s) if !s.is_empty() => {
            let last4 = last4(s);
            (Some(crypto::encrypt(s.as_bytes())?), Some(last4))
        }
        _ => (None, None),
    };
    let notes_enc = match &body.private_notes {
        Some(s) if !s.is_empty() => Some(crypto::encrypt(s.as_bytes())?),
        _ => None,
    };

    let (now, off) = now_utc_naive();
    let saved: Volunteer = diesel::insert_into(volunteers::table)
        .values(NewVolunteer {
            id: Uuid::new_v4(),
            facility_id: body.facility_id,
            full_name: body.full_name.trim().to_string(),
            contact_email: body.contact_email.clone(),
            contact_phone: body.contact_phone.clone(),
            gov_id_encrypted: gov_enc,
            gov_id_last4: gov_last4,
            private_notes_encrypted: notes_enc,
            is_active: true,
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
            facility_id: Some(saved.facility_id),
            entity_type: "volunteer".into(),
            entity_id: saved.id,
            action: "create".into(),
            before_state: None,
            after_state: Some(json!({ "id": saved.id, "fullName": saved.full_name })),
            request_id: ctx.request_id.clone(),
        },
    )?;
    let response = serialize_volunteer(&saved, &ctx);
    idempotency::record_after(pool.get_ref(), &ctx, "POST", "/api/volunteers", 201, &response)?;
    Ok(HttpResponse::Created().json(response))
}

fn last4(s: &str) -> String {
    let n = s.chars().count();
    if n <= 4 {
        s.to_string()
    } else {
        s.chars().skip(n - 4).collect()
    }
}

#[derive(Debug, Deserialize)]
struct ListQuery {
    #[serde(rename = "facilityId")]
    facility_id: Option<Uuid>,
    #[serde(rename = "expiringWithinDays")]
    expiring_within_days: Option<i64>,
}

async fn list_volunteers(
    req: HttpRequest,
    pool: web::Data<DbPool>,
    q: web::Query<ListQuery>,
) -> AppResult<HttpResponse> {
    let ctx = require_ctx(&req)?;
    if !ctx.has_any_permission(&["volunteers.read", "volunteers.write"]) {
        return Err(AppError::Forbidden);
    }
    let mut conn = pool.get()?;
    let mut query = volunteers::table.into_boxed();
    if let Some(set) = ctx.allowed_facilities() {
        let ids: Vec<Uuid> = set.into_iter().collect();
        query = query.filter(volunteers::facility_id.eq_any(ids));
    }
    if let Some(fid) = q.facility_id {
        enforce_scope(&ctx, fid)?;
        query = query.filter(volunteers::facility_id.eq(fid));
    }
    let rows: Vec<Volunteer> = query
        .order(volunteers::created_at.desc())
        .limit(500)
        .load(&mut conn)?;

    if let Some(days) = q.expiring_within_days {
        let today = chrono::Utc::now().date_naive();
        let cutoff = today + Duration::days(days);
        let vol_ids: Vec<Uuid> = rows.iter().map(|v| v.id).collect();
        let expiring: Vec<Qualification> = qualifications::table
            .filter(qualifications::volunteer_id.eq_any(&vol_ids))
            .filter(qualifications::expires_on.is_not_null())
            .filter(qualifications::expires_on.le(cutoff))
            .load(&mut conn)?;
        let with_expiring: std::collections::HashSet<Uuid> =
            expiring.iter().map(|q| q.volunteer_id).collect();
        let filtered: Vec<Volunteer> = rows
            .into_iter()
            .filter(|v| with_expiring.contains(&v.id))
            .collect();
        let out: Vec<Value> = filtered
            .iter()
            .map(|v| serialize_volunteer(v, &ctx))
            .collect();
        return Ok(HttpResponse::Ok().json(json!({
            "volunteers": out,
            "count": out.len(),
            "expiringWithinDays": days,
        })));
    }

    let out: Vec<Value> = rows.iter().map(|v| serialize_volunteer(v, &ctx)).collect();
    Ok(HttpResponse::Ok().json(json!({ "volunteers": out, "count": out.len() })))
}

async fn load_volunteer(pool: &DbPool, id: Uuid) -> AppResult<Volunteer> {
    let mut conn = pool.get()?;
    volunteers::table
        .filter(volunteers::id.eq(id))
        .first(&mut conn)
        .map_err(|e| match e {
            diesel::result::Error::NotFound => AppError::NotFound,
            other => other.into(),
        })
}

async fn get_volunteer(
    req: HttpRequest,
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
) -> AppResult<HttpResponse> {
    let ctx = require_ctx(&req)?;
    if !ctx.has_any_permission(&["volunteers.read", "volunteers.write"]) {
        return Err(AppError::Forbidden);
    }
    let v = load_volunteer(pool.get_ref(), path.into_inner()).await?;
    enforce_scope(&ctx, v.facility_id)?;
    Ok(HttpResponse::Ok().json(serialize_volunteer(&v, &ctx)))
}

#[derive(Debug, Deserialize)]
struct UpdateBody {
    #[serde(rename = "fullName")]
    full_name: Option<String>,
    #[serde(rename = "contactEmail")]
    contact_email: Option<Option<String>>,
    #[serde(rename = "contactPhone")]
    contact_phone: Option<Option<String>>,
    #[serde(rename = "govId")]
    gov_id: Option<Option<String>>,
    #[serde(rename = "privateNotes")]
    private_notes: Option<Option<String>>,
    #[serde(rename = "isActive")]
    is_active: Option<bool>,
}

async fn update_volunteer(
    req: HttpRequest,
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
    body: web::Json<UpdateBody>,
) -> AppResult<HttpResponse> {
    let ctx = require_ctx(&req)?;
    if !ctx.has_any_permission(&["volunteers.write"]) {
        return Err(AppError::Forbidden);
    }
    let id = path.into_inner();
    let v = load_volunteer(pool.get_ref(), id).await?;
    enforce_scope(&ctx, v.facility_id)?;
    // Any attempt to touch gov_id / private_notes (set, clear, or overwrite)
    // requires the field-level allowlist. Without this check a role that has
    // `volunteers.write` but not the allowlist could wipe someone's gov ID.
    enforce_field_patch(&ctx, "gov_id", &body.gov_id)?;
    enforce_field_patch(&ctx, "private_notes", &body.private_notes)?;
    if let Some(replay) = idempotency::check_before(pool.get_ref(), &ctx, &req)? {
        return Ok(replay);
    }
    let mut conn = pool.get()?;
    let (now, off) = now_utc_naive();

    let new_full_name = body.full_name.clone().unwrap_or_else(|| v.full_name.clone());
    let new_contact_email = body.contact_email.clone().unwrap_or(v.contact_email.clone());
    let new_contact_phone = body.contact_phone.clone().unwrap_or(v.contact_phone.clone());

    let (new_gov_enc, new_gov_last4) = match &body.gov_id {
        Some(Some(s)) if !s.is_empty() => (Some(crypto::encrypt(s.as_bytes())?), Some(last4(s))),
        Some(_) => (None, None),
        None => (v.gov_id_encrypted.clone(), v.gov_id_last4.clone()),
    };
    let new_notes_enc = match &body.private_notes {
        Some(Some(s)) if !s.is_empty() => Some(crypto::encrypt(s.as_bytes())?),
        Some(_) => None,
        None => v.private_notes_encrypted.clone(),
    };
    let new_is_active = body.is_active.unwrap_or(v.is_active);

    diesel::update(volunteers::table.filter(volunteers::id.eq(id)))
        .set((
            volunteers::full_name.eq(&new_full_name),
            volunteers::contact_email.eq(new_contact_email),
            volunteers::contact_phone.eq(new_contact_phone),
            volunteers::gov_id_encrypted.eq(new_gov_enc),
            volunteers::gov_id_last4.eq(new_gov_last4),
            volunteers::private_notes_encrypted.eq(new_notes_enc),
            volunteers::is_active.eq(new_is_active),
            volunteers::updated_at.eq(now),
            volunteers::updated_offset_minutes.eq(off),
        ))
        .execute(&mut conn)?;
    let updated = load_volunteer(pool.get_ref(), id).await?;
    audit_svc::write(
        pool.get_ref(),
        audit_svc::AuditEntry {
            actor_user_id: Some(ctx.user.id),
            facility_id: Some(updated.facility_id),
            entity_type: "volunteer".into(),
            entity_id: updated.id,
            action: "update".into(),
            before_state: Some(json!({ "id": v.id, "fullName": v.full_name })),
            after_state: Some(json!({ "id": updated.id, "fullName": updated.full_name })),
            request_id: ctx.request_id.clone(),
        },
    )?;
    let response = serialize_volunteer(&updated, &ctx);
    idempotency::record_after(
        pool.get_ref(),
        &ctx,
        "PUT",
        &format!("/api/volunteers/{}", id),
        200,
        &response,
    )?;
    Ok(HttpResponse::Ok().json(response))
}

async fn delete_volunteer(
    req: HttpRequest,
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
) -> AppResult<HttpResponse> {
    let ctx = require_ctx(&req)?;
    if !ctx.has_any_permission(&["volunteers.write"]) {
        return Err(AppError::Forbidden);
    }
    let id = path.into_inner();
    let v = load_volunteer(pool.get_ref(), id).await?;
    enforce_scope(&ctx, v.facility_id)?;
    if let Some(replay) = idempotency::check_before(pool.get_ref(), &ctx, &req)? {
        return Ok(replay);
    }
    let mut conn = pool.get()?;
    let (now, off) = now_utc_naive();
    diesel::update(volunteers::table.filter(volunteers::id.eq(id)))
        .set((
            volunteers::is_active.eq(false),
            volunteers::updated_at.eq(now),
            volunteers::updated_offset_minutes.eq(off),
        ))
        .execute(&mut conn)?;
    audit_svc::write(
        pool.get_ref(),
        audit_svc::AuditEntry {
            actor_user_id: Some(ctx.user.id),
            facility_id: Some(v.facility_id),
            entity_type: "volunteer".into(),
            entity_id: v.id,
            action: "deactivate".into(),
            before_state: Some(json!({ "id": v.id, "isActive": v.is_active })),
            after_state: Some(json!({ "id": v.id, "isActive": false })),
            request_id: ctx.request_id.clone(),
        },
    )?;
    let response = json!({ "status": "deactivated", "id": id });
    idempotency::record_after(
        pool.get_ref(),
        &ctx,
        "DELETE",
        &format!("/api/volunteers/{}", id),
        200,
        &response,
    )?;
    Ok(HttpResponse::Ok().json(response))
}

#[derive(Debug, Deserialize)]
struct CreateQualBody {
    kind: String,
    issuer: String,
    certificate: Option<String>,
    #[serde(rename = "issuedOn")]
    issued_on: String,
    #[serde(rename = "expiresOn")]
    expires_on: Option<String>,
}

async fn create_qualification(
    req: HttpRequest,
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
    body: web::Json<CreateQualBody>,
) -> AppResult<HttpResponse> {
    let ctx = require_ctx(&req)?;
    if !ctx.has_any_permission(&["volunteers.write"]) {
        return Err(AppError::Forbidden);
    }
    let vid = path.into_inner();
    let v = load_volunteer(pool.get_ref(), vid).await?;
    enforce_scope(&ctx, v.facility_id)?;

    let issued = parse_mdy(&body.issued_on, "issuedOn")?;
    let expires = match &body.expires_on {
        Some(s) if !s.is_empty() => Some(parse_mdy(s, "expiresOn")?),
        _ => None,
    };
    // Certificate is sensitive and encrypted at rest — same field-level rule
    // as gov_id/private_notes.
    enforce_field_write(&ctx, "certificate", body.certificate.as_deref())?;
    if let Some(replay) = idempotency::check_before(pool.get_ref(), &ctx, &req)? {
        return Ok(replay);
    }

    let (cert_enc, cert_last4) = match &body.certificate {
        Some(s) if !s.is_empty() => (Some(crypto::encrypt(s.as_bytes())?), Some(last4(s))),
        _ => (None, None),
    };

    let mut conn = pool.get()?;
    let (now, off) = now_utc_naive();
    let saved: Qualification = diesel::insert_into(qualifications::table)
        .values(NewQualification {
            id: Uuid::new_v4(),
            volunteer_id: v.id,
            kind: body.kind.clone(),
            issuer: body.issuer.clone(),
            certificate_encrypted: cert_enc,
            certificate_last4: cert_last4,
            issued_on: issued,
            expires_on: expires,
            created_at: now,
            created_offset_minutes: off,
        })
        .get_result(&mut conn)?;
    audit_svc::write(
        pool.get_ref(),
        audit_svc::AuditEntry {
            actor_user_id: Some(ctx.user.id),
            facility_id: Some(v.facility_id),
            entity_type: "qualification".into(),
            entity_id: saved.id,
            action: "create".into(),
            before_state: None,
            after_state: Some(json!({
                "id": saved.id,
                "volunteerId": saved.volunteer_id,
                "kind": saved.kind,
                "issuer": saved.issuer,
                "issuedOn": saved.issued_on.format("%m/%d/%Y").to_string(),
                "expiresOn": saved.expires_on.as_ref().map(|d| d.format("%m/%d/%Y").to_string()),
            })),
            request_id: ctx.request_id.clone(),
        },
    )?;

    if let Some(exp) = expires {
        let today = chrono::Utc::now().date_naive();
        if exp - today <= Duration::days(30) {
            let _ = notify::enqueue(
                pool.get_ref(),
                notify::Trigger {
                    user_id: ctx.user.id,
                    event_kind: notify::EVENT_CHANGE,
                    template_code: "volunteer.qualification_expiring".into(),
                    facility_id: Some(v.facility_id),
                    channel: notify::CHANNEL_IN_APP,
                    to_address: None,
                    fallback_subject: format!("Qualification expiring: {}", v.full_name),
                    fallback_body: format!(
                        "Qualification {} expires on {}",
                        saved.kind,
                        exp.format("%m/%d/%Y")
                    ),
                    payload: json!({
                        "volunteer": { "id": v.id, "fullName": v.full_name },
                        "qualification": { "id": saved.id, "kind": saved.kind, "expiresOn": exp.format("%m/%d/%Y").to_string() }
                    }),
                },
            );
        }
    }

    let response = serialize_qual(&saved, &ctx);
    idempotency::record_after(
        pool.get_ref(),
        &ctx,
        "POST",
        &format!("/api/volunteers/{}/qualifications", vid),
        201,
        &response,
    )?;
    Ok(HttpResponse::Created().json(response))
}

fn parse_mdy(s: &str, field: &str) -> AppResult<NaiveDate> {
    crate::services::time::parse_date_mdy(s).ok_or_else(|| AppError::Validation {
        message: format!("{} must be MM/DD/YYYY", field),
        details: json!({ "field": field }),
    })
}

async fn list_qualifications(
    req: HttpRequest,
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
) -> AppResult<HttpResponse> {
    let ctx = require_ctx(&req)?;
    if !ctx.has_any_permission(&["volunteers.read", "volunteers.write"]) {
        return Err(AppError::Forbidden);
    }
    let vid = path.into_inner();
    let v = load_volunteer(pool.get_ref(), vid).await?;
    enforce_scope(&ctx, v.facility_id)?;
    let mut conn = pool.get()?;
    let rows: Vec<Qualification> = qualifications::table
        .filter(qualifications::volunteer_id.eq(vid))
        .order(qualifications::created_at.desc())
        .load(&mut conn)?;
    let out: Vec<Value> = rows.iter().map(|q| serialize_qual(q, &ctx)).collect();
    Ok(HttpResponse::Ok().json(json!({ "qualifications": out, "count": out.len() })))
}

async fn delete_qualification(
    req: HttpRequest,
    pool: web::Data<DbPool>,
    path: web::Path<(Uuid, Uuid)>,
) -> AppResult<HttpResponse> {
    let ctx = require_ctx(&req)?;
    if !ctx.has_any_permission(&["volunteers.write"]) {
        return Err(AppError::Forbidden);
    }
    let (vid, qid) = path.into_inner();
    let v = load_volunteer(pool.get_ref(), vid).await?;
    enforce_scope(&ctx, v.facility_id)?;
    if let Some(replay) = idempotency::check_before(pool.get_ref(), &ctx, &req)? {
        return Ok(replay);
    }
    let mut conn = pool.get()?;
    let affected = diesel::delete(
        qualifications::table
            .filter(qualifications::id.eq(qid))
            .filter(qualifications::volunteer_id.eq(vid)),
    )
    .execute(&mut conn)?;
    if affected == 0 {
        return Err(AppError::NotFound);
    }
    audit_svc::write(
        pool.get_ref(),
        audit_svc::AuditEntry {
            actor_user_id: Some(ctx.user.id),
            facility_id: Some(v.facility_id),
            entity_type: "qualification".into(),
            entity_id: qid,
            action: "delete".into(),
            before_state: None,
            after_state: None,
            request_id: ctx.request_id.clone(),
        },
    )?;
    let response = json!({ "status": "deleted", "id": qid });
    idempotency::record_after(
        pool.get_ref(),
        &ctx,
        "DELETE",
        &format!("/api/volunteers/{}/qualifications/{}", vid, qid),
        200,
        &response,
    )?;
    Ok(HttpResponse::Ok().json(response))
}
