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
use crate::models::audit::AuditLog;
use crate::models::idempotency::IdempotencyKey;
use crate::models::role::{NewPermission, NewRole, Permission, Role};
use crate::models::store::{NewStore, Store};
use crate::models::User;
use crate::schema::{
    audit_logs, idempotency_keys, permissions, role_permissions, roles, stores, user_roles, users,
};
use crate::services::password;
use crate::services::time::now_utc_naive;

pub const SYSADMIN_PERM: &str = "system.admin";

pub fn scope() -> impl HttpServiceFactory {
    web::scope("/admin")
        .wrap(crate::middleware::auth::Authenticate)
        .service(
            web::resource("/users")
                .route(web::post().to(create_user))
                .route(web::get().to(list_users)),
        )
        .service(web::resource("/users/{id}").route(web::put().to(update_user)))
        .route("/users/{id}/unlock", web::put().to(unlock_user))
        .route("/users/{id}/reset-password", web::post().to(reset_password))
        .service(
            web::resource("/roles")
                .route(web::post().to(create_role))
                .route(web::get().to(list_roles)),
        )
        .service(
            web::resource("/roles/{id}")
                .route(web::put().to(update_role))
                .route(web::delete().to(delete_role)),
        )
        .route("/permissions", web::get().to(list_permissions))
        .service(
            web::resource("/facilities")
                .route(web::post().to(create_facility))
                .route(web::get().to(list_facilities)),
        )
        .service(
            web::resource("/facilities/{id}")
                .route(web::put().to(update_facility))
                .route(web::delete().to(delete_facility)),
        )
        .route("/audit/logs", web::get().to(audit_log_list))
        .route("/idempotency/keys", web::get().to(idempotency_keys_list))
}

fn require_sysadmin(req: &HttpRequest) -> AppResult<RequestContext> {
    let ext = req.extensions();
    let ctx = ext
        .get::<RequestContext>()
        .cloned()
        .ok_or(AppError::Unauthenticated)?;
    if !ctx.has_permission(SYSADMIN_PERM) {
        return Err(AppError::Forbidden);
    }
    Ok(ctx)
}

fn serialize_user(u: &User) -> Value {
    json!({
        "id": u.id,
        "username": u.username,
        "displayName": u.display_name,
        "isActive": u.is_active,
        "lockedUntil": u.locked_until.as_ref().map(|t| t.format("%Y-%m-%dT%H:%M:%S").to_string()),
        "createdAt": u.created_at.format("%Y-%m-%dT%H:%M:%S").to_string(),
        "updatedAt": u.updated_at.format("%Y-%m-%dT%H:%M:%S").to_string(),
    })
}

#[derive(Debug, Deserialize)]
struct CreateUserBody {
    username: String,
    password: String,
    #[serde(rename = "displayName")]
    display_name: String,
    #[serde(default, rename = "roleIds")]
    role_ids: Vec<Uuid>,
}

async fn create_user(
    req: HttpRequest,
    pool: web::Data<DbPool>,
    body: web::Json<CreateUserBody>,
) -> AppResult<HttpResponse> {
    let ctx = require_sysadmin(&req)?;
    password::validate_policy(&body.password)?;
    if let Some(replay) = idempotency::check_before(pool.get_ref(), &ctx, &req)? {
        return Ok(replay);
    }
    let hash = password::hash_password(&body.password)?;
    let (now, off) = now_utc_naive();
    let mut conn = pool.get()?;
    let new_user = crate::models::user::NewUser {
        id: Uuid::new_v4(),
        username: body.username.trim().to_string(),
        password_hash: hash,
        display_name: body.display_name.clone(),
        is_active: true,
        locked_until: None,
        created_at: now,
        created_offset_minutes: off,
        updated_at: now,
        updated_offset_minutes: off,
    };
    let saved: User = diesel::insert_into(users::table)
        .values(&new_user)
        .get_result(&mut conn)?;
    for rid in &body.role_ids {
        diesel::insert_into(user_roles::table)
            .values(crate::models::role::UserRole {
                user_id: saved.id,
                role_id: *rid,
            })
            .on_conflict_do_nothing()
            .execute(&mut conn)?;
    }
    crate::services::audit::write(
        pool.get_ref(),
        crate::services::audit::AuditEntry {
            actor_user_id: Some(ctx.user.id),
            facility_id: None,
            entity_type: "user".into(),
            entity_id: saved.id,
            action: "create".into(),
            before_state: None,
            after_state: Some(serialize_user(&saved)),
            request_id: ctx.request_id.clone(),
        },
    )?;
    let response = serialize_user(&saved);
    idempotency::record_after(pool.get_ref(), &ctx, "POST", "/api/admin/users", 201, &response)?;
    Ok(HttpResponse::Created().json(response))
}

async fn list_users(req: HttpRequest, pool: web::Data<DbPool>) -> AppResult<HttpResponse> {
    let _ = require_sysadmin(&req)?;
    let mut conn = pool.get()?;
    let rows: Vec<User> = users::table.order(users::username.asc()).load(&mut conn)?;
    let out: Vec<Value> = rows.iter().map(serialize_user).collect();
    Ok(HttpResponse::Ok().json(json!({ "users": out, "count": out.len() })))
}

#[derive(Debug, Deserialize)]
struct UpdateUserBody {
    #[serde(rename = "displayName")]
    display_name: Option<String>,
    #[serde(rename = "isActive")]
    is_active: Option<bool>,
    #[serde(rename = "roleIds")]
    role_ids: Option<Vec<Uuid>>,
}

async fn update_user(
    req: HttpRequest,
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
    body: web::Json<UpdateUserBody>,
) -> AppResult<HttpResponse> {
    let ctx = require_sysadmin(&req)?;
    if let Some(replay) = idempotency::check_before(pool.get_ref(), &ctx, &req)? {
        return Ok(replay);
    }
    let id = path.into_inner();
    let mut conn = pool.get()?;
    let existing: User = users::table.filter(users::id.eq(id)).first(&mut conn)?;
    let (now, off) = now_utc_naive();
    let new_display = body
        .display_name
        .clone()
        .unwrap_or_else(|| existing.display_name.clone());
    let new_active = body.is_active.unwrap_or(existing.is_active);
    diesel::update(users::table.filter(users::id.eq(id)))
        .set((
            users::display_name.eq(&new_display),
            users::is_active.eq(new_active),
            users::updated_at.eq(now),
            users::updated_offset_minutes.eq(off),
        ))
        .execute(&mut conn)?;
    if let Some(new_roles) = &body.role_ids {
        diesel::delete(user_roles::table.filter(user_roles::user_id.eq(id)))
            .execute(&mut conn)?;
        for rid in new_roles {
            diesel::insert_into(user_roles::table)
                .values(crate::models::role::UserRole {
                    user_id: id,
                    role_id: *rid,
                })
                .on_conflict_do_nothing()
                .execute(&mut conn)?;
        }
    }
    let updated: User = users::table.filter(users::id.eq(id)).first(&mut conn)?;
    crate::services::audit::write(
        pool.get_ref(),
        crate::services::audit::AuditEntry {
            actor_user_id: Some(ctx.user.id),
            facility_id: None,
            entity_type: "user".into(),
            entity_id: updated.id,
            action: "update".into(),
            before_state: Some(serialize_user(&existing)),
            after_state: Some(serialize_user(&updated)),
            request_id: ctx.request_id.clone(),
        },
    )?;
    let response = serialize_user(&updated);
    idempotency::record_after(
        pool.get_ref(),
        &ctx,
        "PUT",
        &format!("/api/admin/users/{}", id),
        200,
        &response,
    )?;
    Ok(HttpResponse::Ok().json(response))
}

async fn unlock_user(
    req: HttpRequest,
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
) -> AppResult<HttpResponse> {
    let ctx = require_sysadmin(&req)?;
    if let Some(replay) = idempotency::check_before(pool.get_ref(), &ctx, &req)? {
        return Ok(replay);
    }
    let id = path.into_inner();
    let mut conn = pool.get()?;
    let username_opt: Option<String> = users::table
        .filter(users::id.eq(id))
        .select(users::username)
        .first(&mut conn)
        .optional()?;
    diesel::update(users::table.filter(users::id.eq(id)))
        .set(users::locked_until.eq::<Option<chrono::NaiveDateTime>>(None))
        .execute(&mut conn)?;
    // Clear recent failed login_attempts so the rolling-window check
    // doesn't re-lock the account on the next login after unlock.
    if let Some(username) = username_opt {
        use crate::schema::login_attempts;
        let (now, _) = crate::services::time::now_utc_naive();
        let window_start = now - chrono::Duration::minutes(15);
        diesel::delete(
            login_attempts::table
                .filter(login_attempts::username.eq(&username))
                .filter(login_attempts::succeeded.eq(false))
                .filter(login_attempts::attempted_at.ge(window_start)),
        )
        .execute(&mut conn)?;
    }
    crate::services::audit::write(
        pool.get_ref(),
        crate::services::audit::AuditEntry {
            actor_user_id: Some(ctx.user.id),
            facility_id: None,
            entity_type: "user".into(),
            entity_id: id,
            action: "unlock".into(),
            before_state: None,
            after_state: None,
            request_id: ctx.request_id.clone(),
        },
    )?;
    let response = json!({ "status": "unlocked", "id": id });
    idempotency::record_after(
        pool.get_ref(),
        &ctx,
        "PUT",
        &format!("/api/admin/users/{}/unlock", id),
        200,
        &response,
    )?;
    Ok(HttpResponse::Ok().json(response))
}

#[derive(Debug, Deserialize)]
struct ResetPwBody {
    #[serde(rename = "newPassword")]
    new_password: String,
}

async fn reset_password(
    req: HttpRequest,
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
    body: web::Json<ResetPwBody>,
) -> AppResult<HttpResponse> {
    let ctx = require_sysadmin(&req)?;
    password::validate_policy(&body.new_password)?;
    if let Some(replay) = idempotency::check_before(pool.get_ref(), &ctx, &req)? {
        return Ok(replay);
    }
    let hash = password::hash_password(&body.new_password)?;
    let id = path.into_inner();
    let mut conn = pool.get()?;
    let (now, off) = now_utc_naive();
    let affected = diesel::update(users::table.filter(users::id.eq(id)))
        .set((
            users::password_hash.eq(&hash),
            users::updated_at.eq(now),
            users::updated_offset_minutes.eq(off),
        ))
        .execute(&mut conn)?;
    if affected == 0 {
        return Err(AppError::NotFound);
    }
    crate::services::audit::write(
        pool.get_ref(),
        crate::services::audit::AuditEntry {
            actor_user_id: Some(ctx.user.id),
            facility_id: None,
            entity_type: "user".into(),
            entity_id: id,
            action: "reset_password".into(),
            before_state: None,
            after_state: None,
            request_id: ctx.request_id.clone(),
        },
    )?;
    let response = json!({ "status": "password_reset", "id": id });
    idempotency::record_after(
        pool.get_ref(),
        &ctx,
        "POST",
        &format!("/api/admin/users/{}/reset-password", id),
        200,
        &response,
    )?;
    Ok(HttpResponse::Ok().json(response))
}

fn serialize_role(r: &Role) -> Value {
    json!({
        "id": r.id,
        "name": r.name,
        "dataScope": r.data_scope,
        "fieldAllowlist": r.field_allowlist,
        "createdAt": r.created_at.format("%Y-%m-%dT%H:%M:%S").to_string(),
    })
}

#[derive(Debug, Deserialize)]
struct RoleBody {
    name: String,
    #[serde(rename = "dataScope")]
    data_scope: String,
    #[serde(default, rename = "fieldAllowlist")]
    field_allowlist: Vec<String>,
    #[serde(default, rename = "permissionCodes")]
    permission_codes: Vec<String>,
}

async fn create_role(
    req: HttpRequest,
    pool: web::Data<DbPool>,
    body: web::Json<RoleBody>,
) -> AppResult<HttpResponse> {
    let ctx = require_sysadmin(&req)?;
    if let Some(replay) = idempotency::check_before(pool.get_ref(), &ctx, &req)? {
        return Ok(replay);
    }
    let mut conn = pool.get()?;
    let (now, off) = now_utc_naive();
    let saved: Role = diesel::insert_into(roles::table)
        .values(NewRole {
            id: Uuid::new_v4(),
            name: body.name.clone(),
            data_scope: body.data_scope.clone(),
            field_allowlist: json!(body.field_allowlist),
            created_at: now,
            created_offset_minutes: off,
        })
        .get_result(&mut conn)?;
    attach_role_permissions(&mut conn, saved.id, &body.permission_codes)?;
    crate::services::audit::write(
        pool.get_ref(),
        crate::services::audit::AuditEntry {
            actor_user_id: Some(ctx.user.id),
            facility_id: None,
            entity_type: "role".into(),
            entity_id: saved.id,
            action: "create".into(),
            before_state: None,
            after_state: Some(serialize_role(&saved)),
            request_id: ctx.request_id.clone(),
        },
    )?;
    let response = serialize_role(&saved);
    idempotency::record_after(pool.get_ref(), &ctx, "POST", "/api/admin/roles", 201, &response)?;
    Ok(HttpResponse::Created().json(response))
}

fn attach_role_permissions(
    conn: &mut diesel::PgConnection,
    role_id: Uuid,
    codes: &[String],
) -> AppResult<()> {
    if codes.is_empty() {
        return Ok(());
    }
    let perm_ids: Vec<Uuid> = permissions::table
        .filter(permissions::code.eq_any(codes))
        .select(permissions::id)
        .load(conn)?;
    for pid in perm_ids {
        diesel::insert_into(role_permissions::table)
            .values(crate::models::role::RolePermission {
                role_id,
                permission_id: pid,
            })
            .on_conflict_do_nothing()
            .execute(conn)?;
    }
    Ok(())
}

async fn list_roles(req: HttpRequest, pool: web::Data<DbPool>) -> AppResult<HttpResponse> {
    let _ = require_sysadmin(&req)?;
    let mut conn = pool.get()?;
    let rows: Vec<Role> = roles::table.order(roles::name.asc()).load(&mut conn)?;
    let out: Vec<Value> = rows.iter().map(serialize_role).collect();
    Ok(HttpResponse::Ok().json(json!({ "roles": out, "count": out.len() })))
}

async fn update_role(
    req: HttpRequest,
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
    body: web::Json<RoleBody>,
) -> AppResult<HttpResponse> {
    let ctx = require_sysadmin(&req)?;
    if let Some(replay) = idempotency::check_before(pool.get_ref(), &ctx, &req)? {
        return Ok(replay);
    }
    let id = path.into_inner();
    let mut conn = pool.get()?;
    let affected = diesel::update(roles::table.filter(roles::id.eq(id)))
        .set((
            roles::name.eq(&body.name),
            roles::data_scope.eq(&body.data_scope),
            roles::field_allowlist.eq(json!(body.field_allowlist)),
        ))
        .execute(&mut conn)?;
    if affected == 0 {
        return Err(AppError::NotFound);
    }
    diesel::delete(role_permissions::table.filter(role_permissions::role_id.eq(id)))
        .execute(&mut conn)?;
    attach_role_permissions(&mut conn, id, &body.permission_codes)?;
    let updated: Role = roles::table.filter(roles::id.eq(id)).first(&mut conn)?;
    crate::services::audit::write(
        pool.get_ref(),
        crate::services::audit::AuditEntry {
            actor_user_id: Some(ctx.user.id),
            facility_id: None,
            entity_type: "role".into(),
            entity_id: updated.id,
            action: "update".into(),
            before_state: None,
            after_state: Some(serialize_role(&updated)),
            request_id: ctx.request_id.clone(),
        },
    )?;
    let response = serialize_role(&updated);
    idempotency::record_after(
        pool.get_ref(),
        &ctx,
        "PUT",
        &format!("/api/admin/roles/{}", id),
        200,
        &response,
    )?;
    Ok(HttpResponse::Ok().json(response))
}

async fn delete_role(
    req: HttpRequest,
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
) -> AppResult<HttpResponse> {
    let ctx = require_sysadmin(&req)?;
    if let Some(replay) = idempotency::check_before(pool.get_ref(), &ctx, &req)? {
        return Ok(replay);
    }
    let id = path.into_inner();
    let mut conn = pool.get()?;
    let affected = diesel::delete(roles::table.filter(roles::id.eq(id))).execute(&mut conn)?;
    if affected == 0 {
        return Err(AppError::NotFound);
    }
    crate::services::audit::write(
        pool.get_ref(),
        crate::services::audit::AuditEntry {
            actor_user_id: Some(ctx.user.id),
            facility_id: None,
            entity_type: "role".into(),
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
        &format!("/api/admin/roles/{}", id),
        200,
        &response,
    )?;
    Ok(HttpResponse::Ok().json(response))
}

async fn list_permissions(req: HttpRequest, pool: web::Data<DbPool>) -> AppResult<HttpResponse> {
    let _ = require_sysadmin(&req)?;
    let mut conn = pool.get()?;
    let rows: Vec<Permission> = permissions::table.order(permissions::code.asc()).load(&mut conn)?;
    let out: Vec<Value> = rows
        .iter()
        .map(|p| json!({ "id": p.id, "code": p.code, "description": p.description }))
        .collect();
    Ok(HttpResponse::Ok().json(json!({ "permissions": out, "count": out.len() })))
}

fn serialize_facility(s: &Store) -> Value {
    json!({
        "id": s.id,
        "name": s.name,
        "code": s.code,
        "isActive": s.is_active,
        "createdAt": s.created_at.format("%Y-%m-%dT%H:%M:%S").to_string(),
    })
}

#[derive(Debug, Deserialize)]
struct FacilityBody {
    name: String,
    code: String,
    #[serde(default = "default_true", rename = "isActive")]
    is_active: bool,
}

fn default_true() -> bool {
    true
}

async fn create_facility(
    req: HttpRequest,
    pool: web::Data<DbPool>,
    body: web::Json<FacilityBody>,
) -> AppResult<HttpResponse> {
    let ctx = require_sysadmin(&req)?;
    if let Some(replay) = idempotency::check_before(pool.get_ref(), &ctx, &req)? {
        return Ok(replay);
    }
    let mut conn = pool.get()?;
    let (now, off) = now_utc_naive();
    let saved: Store = diesel::insert_into(stores::table)
        .values(NewStore {
            id: Uuid::new_v4(),
            name: body.name.clone(),
            code: body.code.clone(),
            is_active: body.is_active,
            created_at: now,
            created_offset_minutes: off,
        })
        .get_result(&mut conn)?;
    crate::services::audit::write(
        pool.get_ref(),
        crate::services::audit::AuditEntry {
            actor_user_id: Some(ctx.user.id),
            facility_id: Some(saved.id),
            entity_type: "facility".into(),
            entity_id: saved.id,
            action: "create".into(),
            before_state: None,
            after_state: Some(serialize_facility(&saved)),
            request_id: ctx.request_id.clone(),
        },
    )?;
    let response = serialize_facility(&saved);
    idempotency::record_after(
        pool.get_ref(),
        &ctx,
        "POST",
        "/api/admin/facilities",
        201,
        &response,
    )?;
    Ok(HttpResponse::Created().json(response))
}

async fn list_facilities(req: HttpRequest, pool: web::Data<DbPool>) -> AppResult<HttpResponse> {
    let _ = require_sysadmin(&req)?;
    let mut conn = pool.get()?;
    let rows: Vec<Store> = stores::table.order(stores::code.asc()).load(&mut conn)?;
    let out: Vec<Value> = rows.iter().map(serialize_facility).collect();
    Ok(HttpResponse::Ok().json(json!({ "facilities": out, "count": out.len() })))
}

async fn update_facility(
    req: HttpRequest,
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
    body: web::Json<FacilityBody>,
) -> AppResult<HttpResponse> {
    let ctx = require_sysadmin(&req)?;
    if let Some(replay) = idempotency::check_before(pool.get_ref(), &ctx, &req)? {
        return Ok(replay);
    }
    let id = path.into_inner();
    let mut conn = pool.get()?;
    let affected = diesel::update(stores::table.filter(stores::id.eq(id)))
        .set((
            stores::name.eq(&body.name),
            stores::code.eq(&body.code),
            stores::is_active.eq(body.is_active),
        ))
        .execute(&mut conn)?;
    if affected == 0 {
        return Err(AppError::NotFound);
    }
    let updated: Store = stores::table.filter(stores::id.eq(id)).first(&mut conn)?;
    crate::services::audit::write(
        pool.get_ref(),
        crate::services::audit::AuditEntry {
            actor_user_id: Some(ctx.user.id),
            facility_id: Some(updated.id),
            entity_type: "facility".into(),
            entity_id: updated.id,
            action: "update".into(),
            before_state: None,
            after_state: Some(serialize_facility(&updated)),
            request_id: ctx.request_id.clone(),
        },
    )?;
    let response = serialize_facility(&updated);
    idempotency::record_after(
        pool.get_ref(),
        &ctx,
        "PUT",
        &format!("/api/admin/facilities/{}", id),
        200,
        &response,
    )?;
    Ok(HttpResponse::Ok().json(response))
}

async fn delete_facility(
    req: HttpRequest,
    pool: web::Data<DbPool>,
    path: web::Path<Uuid>,
) -> AppResult<HttpResponse> {
    let ctx = require_sysadmin(&req)?;
    if let Some(replay) = idempotency::check_before(pool.get_ref(), &ctx, &req)? {
        return Ok(replay);
    }
    let id = path.into_inner();
    let mut conn = pool.get()?;
    let affected = diesel::update(stores::table.filter(stores::id.eq(id)))
        .set(stores::is_active.eq(false))
        .execute(&mut conn)?;
    if affected == 0 {
        return Err(AppError::NotFound);
    }
    crate::services::audit::write(
        pool.get_ref(),
        crate::services::audit::AuditEntry {
            actor_user_id: Some(ctx.user.id),
            facility_id: Some(id),
            entity_type: "facility".into(),
            entity_id: id,
            action: "deactivate".into(),
            before_state: None,
            after_state: None,
            request_id: ctx.request_id.clone(),
        },
    )?;
    let response = json!({ "status": "deactivated", "id": id });
    idempotency::record_after(
        pool.get_ref(),
        &ctx,
        "DELETE",
        &format!("/api/admin/facilities/{}", id),
        200,
        &response,
    )?;
    Ok(HttpResponse::Ok().json(response))
}

#[derive(Debug, Deserialize)]
struct AuditQuery {
    #[serde(rename = "entityType")]
    entity_type: Option<String>,
    #[serde(rename = "entityId")]
    entity_id: Option<Uuid>,
    action: Option<String>,
    limit: Option<i64>,
}

async fn audit_log_list(
    req: HttpRequest,
    pool: web::Data<DbPool>,
    q: web::Query<AuditQuery>,
) -> AppResult<HttpResponse> {
    let _ = require_sysadmin(&req)?;
    let mut conn = pool.get()?;
    let limit = q.limit.unwrap_or(200).clamp(1, 1000);
    let mut query = audit_logs::table.into_boxed();
    if let Some(et) = &q.entity_type {
        query = query.filter(audit_logs::entity_type.eq(et));
    }
    if let Some(eid) = q.entity_id {
        query = query.filter(audit_logs::entity_id.eq(eid));
    }
    if let Some(a) = &q.action {
        query = query.filter(audit_logs::action.eq(a));
    }
    let rows: Vec<AuditLog> = query
        .order(audit_logs::created_at.desc())
        .limit(limit)
        .load(&mut conn)?;
    let out: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "id": r.id,
                "actorUserId": r.actor_user_id,
                "facilityId": r.facility_id,
                "entityType": r.entity_type,
                "entityId": r.entity_id,
                "action": r.action,
                "before": r.before_state,
                "after": r.after_state,
                "requestId": r.request_id,
                "createdAt": r.created_at.format("%Y-%m-%dT%H:%M:%S").to_string(),
            })
        })
        .collect();
    Ok(HttpResponse::Ok().json(json!({ "logs": out, "count": out.len() })))
}

async fn idempotency_keys_list(
    req: HttpRequest,
    pool: web::Data<DbPool>,
) -> AppResult<HttpResponse> {
    let _ = require_sysadmin(&req)?;
    let mut conn = pool.get()?;
    let (now, _) = now_utc_naive();
    let rows: Vec<IdempotencyKey> = idempotency_keys::table
        .filter(idempotency_keys::expires_at.gt(now))
        .order(idempotency_keys::created_at.desc())
        .limit(200)
        .load(&mut conn)?;
    let out: Vec<Value> = rows
        .iter()
        .map(|k| {
            json!({
                "id": k.id,
                "userId": k.user_id,
                "requestId": k.request_id,
                "method": k.method,
                "path": k.path,
                "statusCode": k.status_code,
                "createdAt": k.created_at.format("%Y-%m-%dT%H:%M:%S").to_string(),
                "expiresAt": k.expires_at.format("%Y-%m-%dT%H:%M:%S").to_string(),
            })
        })
        .collect();
    Ok(HttpResponse::Ok().json(json!({ "keys": out, "count": out.len() })))
}

// Silence unused warnings on helpers exposed only via macros.
#[allow(dead_code)]
fn _keep<T>(_: T) {}

#[allow(dead_code)]
fn _ensure_model_imports() {
    let _: NewPermission = NewPermission {
        id: Uuid::nil(),
        code: String::new(),
        description: String::new(),
    };
}
