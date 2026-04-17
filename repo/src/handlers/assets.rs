use actix_web::dev::HttpServiceFactory;
use actix_web::{web, HttpMessage, HttpRequest, HttpResponse};
use diesel::prelude::*;
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::db::DbPool;
use crate::errors::{AppError, AppResult};
use crate::middleware::idempotency;
use crate::middleware::request_context::RequestContext;
use crate::models::asset::{Asset, AssetEvent, MaintenanceRecord, NewAsset, NewAssetEvent, NewMaintenanceRecord};
use crate::schema::{asset_events, assets, maintenance_records, stores};
use crate::services::audit as audit_svc;
use crate::services::time::now_utc_naive;

pub const STATE_INTAKE: &str = "INTAKE";
pub const STATE_ASSIGNMENT: &str = "ASSIGNMENT";
pub const STATE_LOAN: &str = "LOAN";
pub const STATE_TRANSFER: &str = "TRANSFER";
pub const STATE_MAINTENANCE: &str = "MAINTENANCE";
pub const STATE_REPAIR: &str = "REPAIR";
pub const STATE_INVENTORY_COUNT: &str = "INVENTORY_COUNT";
pub const STATE_DISPOSAL: &str = "DISPOSAL";

pub const ALL_STATES: &[&str] = &[
    STATE_INTAKE,
    STATE_ASSIGNMENT,
    STATE_LOAN,
    STATE_TRANSFER,
    STATE_MAINTENANCE,
    STATE_REPAIR,
    STATE_INVENTORY_COUNT,
    STATE_DISPOSAL,
];

pub const MAX_BULK_IDS: usize = 500;

pub fn scope() -> impl HttpServiceFactory {
    web::scope("/assets")
        .wrap(crate::middleware::auth::Authenticate)
        .service(
            web::resource("")
                .route(web::post().to(create_asset))
                .route(web::get().to(list_assets)),
        )
        // Static paths must be registered BEFORE dynamic /{id} routes,
        // otherwise Actix matches "bulk-transition" as an {id} segment.
        .route("/bulk-transition", web::post().to(bulk_transition))
        .service(web::resource("/{id}").route(web::get().to(get_asset)))
        .route("/{id}/history", web::get().to(asset_history))
        .route("/{id}/transition", web::post().to(single_transition))
        .service(
            web::resource("/{id}/maintenance-records")
                .route(web::get().to(list_maintenance))
                .route(web::post().to(create_maintenance)),
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

/// Pure validator for state transitions. Used by both single and bulk paths.
pub fn validate_transition(from: &str, to: &str) -> Result<(), AppError> {
    if !ALL_STATES.contains(&from) || !ALL_STATES.contains(&to) {
        return Err(AppError::InvalidTransition(format!(
            "unknown state {} -> {}",
            from, to
        )));
    }
    if from == STATE_DISPOSAL {
        return Err(AppError::InvalidTransition(
            "DISPOSAL is terminal".into(),
        ));
    }
    let ok = match from {
        STATE_INTAKE => matches!(to, STATE_ASSIGNMENT | STATE_INVENTORY_COUNT),
        STATE_ASSIGNMENT => {
            matches!(to, STATE_LOAN | STATE_TRANSFER | STATE_MAINTENANCE | STATE_INVENTORY_COUNT)
        }
        STATE_LOAN => matches!(to, STATE_ASSIGNMENT | STATE_MAINTENANCE),
        STATE_TRANSFER => matches!(to, STATE_ASSIGNMENT),
        STATE_MAINTENANCE => matches!(to, STATE_REPAIR | STATE_ASSIGNMENT),
        STATE_REPAIR => matches!(to, STATE_ASSIGNMENT | STATE_DISPOSAL),
        // INVENTORY_COUNT allows return to prior state (handled by caller).
        STATE_INVENTORY_COUNT => false,
        _ => false,
    };
    if ok {
        Ok(())
    } else {
        Err(AppError::InvalidTransition(format!(
            "{} -> {} not allowed",
            from, to
        )))
    }
}

fn serialize_asset(a: &Asset) -> Value {
    json!({
        "id": a.id,
        "facilityId": a.facility_id,
        "assetLabel": a.asset_label,
        "name": a.name,
        "status": a.status,
        "priorStatus": a.prior_status,
        "description": a.description,
        "createdAt": a.created_at.format("%Y-%m-%dT%H:%M:%S").to_string(),
        "updatedAt": a.updated_at.format("%Y-%m-%dT%H:%M:%S").to_string(),
    })
}

#[derive(Debug, Deserialize)]
struct CreateBody {
    #[serde(rename = "facilityId")]
    facility_id: Uuid,
    #[serde(rename = "assetLabel")]
    asset_label: String,
    name: String,
    #[serde(default)]
    description: String,
}

async fn create_asset(
    req: HttpRequest,
    pool: web::Data<DbPool>,
    body: web::Json<CreateBody>,
) -> AppResult<HttpResponse> {
    let ctx = require_ctx(&req)?;
    if !ctx.has_any_permission(&["assets.write"]) {
        return Err(AppError::Forbidden);
    }
    enforce_facility_scope(&ctx, body.facility_id)?;
    if body.asset_label.trim().is_empty() {
        return Err(AppError::Validation {
            message: "assetLabel required".into(),
            details: json!({ "field": "assetLabel" }),
        });
    }
    if body.name.trim().is_empty() {
        return Err(AppError::Validation {
            message: "name required".into(),
            details: json!({ "field": "name" }),
        });
    }
    if let Some(replay) = idempotency::check_before(pool.get_ref(), &ctx, &req)? {
        return Ok(replay);
    }
    let mut conn = pool.get()?;
    let exists: Option<Uuid> = stores::table
        .filter(stores::id.eq(body.facility_id))
        .select(stores::id)
        .first(&mut conn)
        .optional()?;
    if exists.is_none() {
        return Err(AppError::Validation {
            message: "facility not found".into(),
            details: json!({ "field": "facilityId" }),
        });
    }
    let (now, off) = now_utc_naive();
    let new = NewAsset {
        id: Uuid::new_v4(),
        facility_id: body.facility_id,
        asset_label: body.asset_label.trim().to_string(),
        name: body.name.trim().to_string(),
        status: STATE_INTAKE.to_string(),
        prior_status: None,
        description: body.description.clone(),
        acquired_at: Some(now),
        acquired_offset_minutes: Some(off),
        created_at: now,
        created_offset_minutes: off,
        updated_at: now,
        updated_offset_minutes: off,
    };
    let saved: Asset = diesel::insert_into(assets::table)
        .values(&new)
        .get_result(&mut conn)?;
    diesel::insert_into(asset_events::table)
        .values(&NewAssetEvent {
            id: Uuid::new_v4(),
            asset_id: saved.id,
            from_status: None,
            to_status: STATE_INTAKE.to_string(),
            actor_user_id: ctx.user.id,
            note: "created".into(),
            created_at: now,
            created_offset_minutes: off,
        })
        .execute(&mut conn)?;
    audit_svc::write(
        pool.get_ref(),
        audit_svc::AuditEntry {
            actor_user_id: Some(ctx.user.id),
            facility_id: Some(saved.facility_id),
            entity_type: "asset".into(),
            entity_id: saved.id,
            action: "create".into(),
            before_state: None,
            after_state: Some(serialize_asset(&saved)),
            request_id: ctx.request_id.clone(),
        },
    )?;
    let response = serialize_asset(&saved);
    idempotency::record_after(pool.get_ref(), &ctx, "POST", "/api/assets", 201, &response)?;
    Ok(HttpResponse::Created().json(response))
}

#[derive(Debug, Deserialize)]
struct ListQuery {
    #[serde(rename = "facilityId")]
    facility_id: Option<Uuid>,
    status: Option<String>,
}

async fn list_assets(
    req: HttpRequest,
    pool: web::Data<DbPool>,
    q: web::Query<ListQuery>,
) -> AppResult<HttpResponse> {
    let ctx = require_ctx(&req)?;
    if !ctx.has_any_permission(&["assets.read", "assets.write", "assets.transition"]) {
        return Err(AppError::Forbidden);
    }
    let mut conn = pool.get()?;
    let mut query = assets::table.into_boxed();
    if let Some(set) = ctx.allowed_facilities() {
        let ids: Vec<Uuid> = set.into_iter().collect();
        query = query.filter(assets::facility_id.eq_any(ids));
    }
    if let Some(fid) = q.facility_id {
        enforce_facility_scope(&ctx, fid)?;
        query = query.filter(assets::facility_id.eq(fid));
    }
    if let Some(s) = &q.status {
        query = query.filter(assets::status.eq(s));
    }
    let rows: Vec<Asset> = query
        .order(assets::created_at.desc())
        .limit(500)
        .load(&mut conn)?;
    let out: Vec<Value> = rows.iter().map(serialize_asset).collect();
    Ok(HttpResponse::Ok().json(json!({ "assets": out, "count": out.len() })))
}

async fn load_asset(pool: &DbPool, id: Uuid) -> AppResult<Asset> {
    let mut conn = pool.get()?;
    assets::table
        .filter(assets::id.eq(id))
        .first(&mut conn)
        .map_err(|e| match e {
            diesel::result::Error::NotFound => AppError::NotFound,
            other => other.into(),
        })
}

async fn get_asset(
    req: HttpRequest,
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
) -> AppResult<HttpResponse> {
    let ctx = require_ctx(&req)?;
    if !ctx.has_any_permission(&["assets.read", "assets.write", "assets.transition"]) {
        return Err(AppError::Forbidden);
    }
    let a = load_asset(pool.get_ref(), path.into_inner()).await?;
    enforce_facility_scope(&ctx, a.facility_id)?;
    Ok(HttpResponse::Ok().json(serialize_asset(&a)))
}

async fn asset_history(
    req: HttpRequest,
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
) -> AppResult<HttpResponse> {
    let ctx = require_ctx(&req)?;
    if !ctx.has_any_permission(&["assets.read", "assets.write", "assets.transition"]) {
        return Err(AppError::Forbidden);
    }
    let id = path.into_inner();
    let a = load_asset(pool.get_ref(), id).await?;
    enforce_facility_scope(&ctx, a.facility_id)?;
    let mut conn = pool.get()?;
    let evs: Vec<AssetEvent> = asset_events::table
        .filter(asset_events::asset_id.eq(id))
        .order(asset_events::created_at.asc())
        .load(&mut conn)?;
    let out: Vec<Value> = evs
        .iter()
        .map(|e| {
            json!({
                "id": e.id,
                "fromStatus": e.from_status,
                "toStatus": e.to_status,
                "actorUserId": e.actor_user_id,
                "note": e.note,
                "createdAt": e.created_at.format("%Y-%m-%dT%H:%M:%S").to_string(),
            })
        })
        .collect();
    Ok(HttpResponse::Ok().json(json!({ "events": out, "count": out.len() })))
}

#[derive(Debug, Deserialize)]
struct TransitionBody {
    #[serde(rename = "toState")]
    to_state: String,
    #[serde(default)]
    note: String,
}

async fn single_transition(
    req: HttpRequest,
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
    body: web::Json<TransitionBody>,
) -> AppResult<HttpResponse> {
    let ctx = require_ctx(&req)?;
    if !ctx.has_any_permission(&["assets.transition"]) {
        return Err(AppError::Forbidden);
    }
    let id = path.into_inner();
    let asset = load_asset(pool.get_ref(), id).await?;
    enforce_facility_scope(&ctx, asset.facility_id)?;
    if let Some(replay) = idempotency::check_before(pool.get_ref(), &ctx, &req)? {
        return Ok(replay);
    }
    let to = body.to_state.trim().to_string();
    apply_transition(pool.get_ref(), &ctx, &asset, &to, &body.note)?;
    let updated = load_asset(pool.get_ref(), id).await?;
    let response = serialize_asset(&updated);
    idempotency::record_after(
        pool.get_ref(),
        &ctx,
        "POST",
        &format!("/api/assets/{}/transition", id),
        200,
        &response,
    )?;
    Ok(HttpResponse::Ok().json(response))
}

fn apply_transition(
    pool: &DbPool,
    ctx: &RequestContext,
    asset: &Asset,
    to: &str,
    note: &str,
) -> AppResult<()> {
    // INVENTORY_COUNT has special back-to-prior semantics; for forward into inventory we
    // record prior_status and for leaving we require `to` equals prior_status.
    let from = asset.status.as_str();
    if from == STATE_INVENTORY_COUNT {
        match &asset.prior_status {
            Some(prior) if prior == to => {}
            _ => {
                return Err(AppError::InvalidTransition(
                    "from INVENTORY_COUNT must return to prior state".into(),
                ));
            }
        }
    } else {
        validate_transition(from, to)?;
    }

    let mut conn = pool.get()?;
    let (now, off) = now_utc_naive();
    let before = serialize_asset(asset);
    let new_prior = if to == STATE_INVENTORY_COUNT {
        Some(asset.status.clone())
    } else {
        None
    };
    diesel::update(assets::table.filter(assets::id.eq(asset.id)))
        .set((
            assets::status.eq(to),
            assets::prior_status.eq(new_prior),
            assets::updated_at.eq(now),
            assets::updated_offset_minutes.eq(off),
        ))
        .execute(&mut conn)?;
    diesel::insert_into(asset_events::table)
        .values(NewAssetEvent {
            id: Uuid::new_v4(),
            asset_id: asset.id,
            from_status: Some(asset.status.clone()),
            to_status: to.to_string(),
            actor_user_id: ctx.user.id,
            note: note.to_string(),
            created_at: now,
            created_offset_minutes: off,
        })
        .execute(&mut conn)?;
    let after: Asset = assets::table.filter(assets::id.eq(asset.id)).first(&mut conn)?;
    audit_svc::write(
        pool,
        audit_svc::AuditEntry {
            actor_user_id: Some(ctx.user.id),
            facility_id: Some(asset.facility_id),
            entity_type: "asset".into(),
            entity_id: asset.id,
            action: "transition".into(),
            before_state: Some(before),
            after_state: Some(serialize_asset(&after)),
            request_id: ctx.request_id.clone(),
        },
    )?;
    Ok(())
}

#[derive(Debug, Deserialize)]
struct BulkTransitionBody {
    ids: Vec<Uuid>,
    #[serde(rename = "toState")]
    to_state: String,
    #[serde(default)]
    note: String,
}

async fn bulk_transition(
    req: HttpRequest,
    pool: web::Data<DbPool>,
    body: web::Json<BulkTransitionBody>,
) -> AppResult<HttpResponse> {
    let ctx = require_ctx(&req)?;
    if !ctx.has_any_permission(&["assets.transition"]) {
        return Err(AppError::Forbidden);
    }
    if body.ids.len() > MAX_BULK_IDS {
        return Err(AppError::Validation {
            message: format!("at most {} ids per request", MAX_BULK_IDS),
            details: json!({ "field": "ids", "limit": MAX_BULK_IDS }),
        });
    }
    if !ALL_STATES.contains(&body.to_state.as_str()) {
        return Err(AppError::Validation {
            message: "unknown toState".into(),
            details: json!({ "field": "toState" }),
        });
    }

    if let Some(replay) = idempotency::check_before(pool.get_ref(), &ctx, &req)? {
        return Ok(replay);
    }

    let mut conn = pool.get()?;
    let fetched: Vec<Asset> = assets::table
        .filter(assets::id.eq_any(&body.ids))
        .load(&mut conn)?;
    let allowed = ctx.allowed_facilities();
    let mut rejected: Vec<Value> = Vec::new();
    let mut to_commit: Vec<Asset> = Vec::new();
    let fetched_ids: std::collections::HashSet<Uuid> = fetched.iter().map(|a| a.id).collect();
    for id in &body.ids {
        if !fetched_ids.contains(id) {
            rejected.push(json!({ "id": id, "reason": "not_found" }));
        }
    }
    for a in fetched {
        match &allowed {
            None => {}
            Some(set) if set.contains(&a.facility_id) => {}
            _ => {
                rejected.push(json!({ "id": a.id, "reason": "out_of_scope" }));
                continue;
            }
        }
        let from = a.status.as_str();
        let valid = if from == STATE_INVENTORY_COUNT {
            matches!(&a.prior_status, Some(p) if p == &body.to_state)
        } else {
            validate_transition(from, &body.to_state).is_ok()
        };
        if !valid {
            rejected.push(json!({
                "id": a.id,
                "reason": "invalid_transition",
                "from": from,
                "to": body.to_state,
            }));
            continue;
        }
        to_commit.push(a);
    }

    let (now, off) = now_utc_naive();
    let committed_ids: Vec<Uuid> = to_commit.iter().map(|a| a.id).collect();
    conn.transaction::<(), diesel::result::Error, _>(|tx| {
        for a in &to_commit {
            let new_prior = if body.to_state == STATE_INVENTORY_COUNT {
                Some(a.status.clone())
            } else {
                None
            };
            diesel::update(assets::table.filter(assets::id.eq(a.id)))
                .set((
                    assets::status.eq(&body.to_state),
                    assets::prior_status.eq(new_prior),
                    assets::updated_at.eq(now),
                    assets::updated_offset_minutes.eq(off),
                ))
                .execute(tx)?;
            diesel::insert_into(asset_events::table)
                .values(NewAssetEvent {
                    id: Uuid::new_v4(),
                    asset_id: a.id,
                    from_status: Some(a.status.clone()),
                    to_status: body.to_state.clone(),
                    actor_user_id: ctx.user.id,
                    note: body.note.clone(),
                    created_at: now,
                    created_offset_minutes: off,
                })
                .execute(tx)?;
        }
        Ok(())
    })?;

    for a in &to_commit {
        audit_svc::write(
            pool.get_ref(),
            audit_svc::AuditEntry {
                actor_user_id: Some(ctx.user.id),
                facility_id: Some(a.facility_id),
                entity_type: "asset".into(),
                entity_id: a.id,
                action: "bulk_transition".into(),
                before_state: Some(serialize_asset(a)),
                after_state: Some(json!({ "id": a.id, "status": body.to_state })),
                request_id: ctx.request_id.clone(),
            },
        )?;
    }

    let response_body = json!({
        "committed": committed_ids.len(),
        "committedIds": committed_ids,
        "rejected": rejected,
        "toState": body.to_state,
    });
    idempotency::record_after(
        pool.get_ref(),
        &ctx,
        "POST",
        "/api/assets/bulk-transition",
        200,
        &response_body,
    )?;
    Ok(HttpResponse::Ok().json(response_body))
}

#[derive(Debug, Deserialize)]
struct MaintenanceBody {
    summary: String,
    #[serde(default)]
    details: String,
}

async fn create_maintenance(
    req: HttpRequest,
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
    body: web::Json<MaintenanceBody>,
) -> AppResult<HttpResponse> {
    let ctx = require_ctx(&req)?;
    if !ctx.has_any_permission(&["assets.write"]) {
        return Err(AppError::Forbidden);
    }
    if body.summary.trim().is_empty() {
        return Err(AppError::Validation {
            message: "summary required".into(),
            details: json!({ "field": "summary" }),
        });
    }
    let id = path.into_inner();
    let asset = load_asset(pool.get_ref(), id).await?;
    enforce_facility_scope(&ctx, asset.facility_id)?;
    if let Some(replay) = idempotency::check_before(pool.get_ref(), &ctx, &req)? {
        return Ok(replay);
    }
    let mut conn = pool.get()?;
    let (now, off) = now_utc_naive();
    let saved: MaintenanceRecord = diesel::insert_into(maintenance_records::table)
        .values(NewMaintenanceRecord {
            id: Uuid::new_v4(),
            asset_id: asset.id,
            performed_at: now,
            performed_offset_minutes: off,
            performed_by: ctx.user.id,
            summary: body.summary.clone(),
            details: body.details.clone(),
            created_at: now,
            created_offset_minutes: off,
        })
        .get_result(&mut conn)?;
    let response = json!({
        "id": saved.id,
        "assetId": saved.asset_id,
        "summary": saved.summary,
        "details": saved.details,
        "performedAt": saved.performed_at.format("%Y-%m-%dT%H:%M:%S").to_string(),
    });
    idempotency::record_after(
        pool.get_ref(),
        &ctx,
        "POST",
        &format!("/api/assets/{}/maintenance-records", id),
        201,
        &response,
    )?;
    Ok(HttpResponse::Created().json(response))
}

async fn list_maintenance(
    req: HttpRequest,
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
) -> AppResult<HttpResponse> {
    let ctx = require_ctx(&req)?;
    if !ctx.has_any_permission(&["assets.read", "assets.write", "assets.transition"]) {
        return Err(AppError::Forbidden);
    }
    let id = path.into_inner();
    let asset = load_asset(pool.get_ref(), id).await?;
    enforce_facility_scope(&ctx, asset.facility_id)?;
    let mut conn = pool.get()?;
    let rows: Vec<MaintenanceRecord> = maintenance_records::table
        .filter(maintenance_records::asset_id.eq(id))
        .order(maintenance_records::performed_at.desc())
        .load(&mut conn)?;
    let out: Vec<Value> = rows
        .iter()
        .map(|m| {
            json!({
                "id": m.id,
                "assetId": m.asset_id,
                "summary": m.summary,
                "details": m.details,
                "performedAt": m.performed_at.format("%Y-%m-%dT%H:%M:%S").to_string(),
            })
        })
        .collect();
    Ok(HttpResponse::Ok().json(json!({ "records": out, "count": out.len() })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transition_table_sampling() {
        assert!(validate_transition(STATE_INTAKE, STATE_ASSIGNMENT).is_ok());
        assert!(validate_transition(STATE_INTAKE, STATE_INVENTORY_COUNT).is_ok());
        assert!(validate_transition(STATE_INTAKE, STATE_LOAN).is_err());
        assert!(validate_transition(STATE_ASSIGNMENT, STATE_LOAN).is_ok());
        assert!(validate_transition(STATE_LOAN, STATE_ASSIGNMENT).is_ok());
        assert!(validate_transition(STATE_LOAN, STATE_DISPOSAL).is_err());
        assert!(validate_transition(STATE_MAINTENANCE, STATE_REPAIR).is_ok());
        assert!(validate_transition(STATE_REPAIR, STATE_DISPOSAL).is_ok());
        assert!(validate_transition(STATE_DISPOSAL, STATE_ASSIGNMENT).is_err());
    }
}
