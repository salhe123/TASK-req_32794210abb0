use actix_web::dev::HttpServiceFactory;
use actix_web::{web, HttpMessage, HttpRequest, HttpResponse};
use chrono::Duration;
use diesel::prelude::*;
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::config::Config;
use crate::db::DbPool;
use crate::errors::{AppError, AppResult};
use crate::middleware::idempotency;
use crate::middleware::request_context::RequestContext;
use crate::models::session::NewLoginAttempt;
use crate::models::User;
use crate::schema::{login_attempts, users};
use crate::services::rate_limit;
use crate::services::{password, session as session_svc};
use crate::services::time::now_utc_naive;

pub fn scope() -> impl HttpServiceFactory {
    web::scope("/auth")
        .service(web::resource("/login").route(web::post().to(login)))
        .service(
            web::resource("/logout")
                .wrap(crate::middleware::auth::Authenticate)
                .route(web::post().to(logout)),
        )
        .service(
            web::resource("/change-password")
                .wrap(crate::middleware::auth::Authenticate)
                .route(web::post().to(change_password)),
        )
        .service(
            web::resource("/session")
                .wrap(crate::middleware::auth::Authenticate)
                .route(web::get().to(session_info)),
        )
}

const LOCKOUT_THRESHOLD: i64 = 5;
const LOCKOUT_WINDOW_MIN: i64 = 15;

#[derive(Debug, Deserialize)]
struct LoginBody {
    username: String,
    password: String,
}

/// Read the `X-Request-Id` header for login idempotency.
/// The Authenticate middleware does not run on this route, so we extract
/// the header by hand. The prompt's "all writes idempotent" rule applies to
/// login too — a retry of the same request_id by the same (eventually
/// authenticated) user must replay the originally-issued token rather than
/// mint a second session.
fn extract_login_request_id(req: &HttpRequest) -> AppResult<String> {
    let raw = req
        .headers()
        .get("X-Request-Id")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| AppError::Validation {
            message: "X-Request-Id header is required for login".into(),
            details: json!({ "field": "X-Request-Id" }),
        })?;
    uuid::Uuid::parse_str(raw).map_err(|_| AppError::Validation {
        message: "X-Request-Id must be a UUID".into(),
        details: json!({ "field": "X-Request-Id" }),
    })?;
    Ok(raw.to_string())
}

async fn login(
    req: HttpRequest,
    pool: web::Data<DbPool>,
    cfg: web::Data<Config>,
    body: web::Json<LoginBody>,
) -> AppResult<HttpResponse> {
    let peer = req
        .peer_addr()
        .map(|a| a.ip().to_string())
        .unwrap_or_else(|| "unknown".into());
    if !rate_limit::check(&format!("login:{}", peer)) {
        return Err(AppError::RateLimited);
    }

    // Validate request_id format up front — a malformed header is rejected
    // without revealing whether the username exists.
    let request_id = extract_login_request_id(&req)?;

    let username = body.username.trim().to_string();
    if username.is_empty() || body.password.is_empty() {
        return Err(AppError::Validation {
            message: "username and password are required".into(),
            details: json!({}),
        });
    }

    let mut conn = pool.get()?;
    let (now, off) = now_utc_naive();

    let user_opt: Option<User> = users::table
        .filter(users::username.eq(&username))
        .first::<User>(&mut conn)
        .optional()?;

    let window_start = now - Duration::minutes(LOCKOUT_WINDOW_MIN);
    let recent_failures: i64 = login_attempts::table
        .filter(login_attempts::username.eq(&username))
        .filter(login_attempts::succeeded.eq(false))
        .filter(login_attempts::attempted_at.ge(window_start))
        .count()
        .get_result(&mut conn)?;

    if let Some(user) = &user_opt {
        if let Some(until) = user.locked_until {
            if until > now {
                return Err(AppError::AccountLocked);
            }
        }
    }

    if recent_failures >= LOCKOUT_THRESHOLD {
        if let Some(user) = &user_opt {
            diesel::update(users::table.filter(users::id.eq(user.id)))
                .set(users::locked_until.eq(now + Duration::minutes(LOCKOUT_WINDOW_MIN)))
                .execute(&mut conn)?;
        }
        record_attempt(&mut conn, &username, false, now, off)?;
        return Err(AppError::AccountLocked);
    }

    let user = match user_opt {
        Some(u) => u,
        None => {
            record_attempt(&mut conn, &username, false, now, off)?;
            return Err(AppError::Validation {
                message: "invalid credentials".into(),
                details: json!({}),
            });
        }
    };

    if !user.is_active {
        record_attempt(&mut conn, &username, false, now, off)?;
        return Err(AppError::Forbidden);
    }

    if !password::verify_password(&body.password, &user.password_hash) {
        record_attempt(&mut conn, &username, false, now, off)?;
        return Err(AppError::Validation {
            message: "invalid credentials".into(),
            details: json!({}),
        });
    }

    // Password has been verified. Now enforce idempotency for the login write.
    // The replay key includes user.id which is only known post-auth; this is why
    // the check happens here rather than at the top of the handler.
    //
    // Cross-user reuse of the same request_id → 409 IdempotencyConflict.
    // Same-user replay → the cached response body (i.e. the originally-issued
    // token) is returned and no new session is minted. A retried login due to a
    // dropped ACK therefore does not accumulate sessions.
    if let Some(existing) = idempotency::lookup(
        pool.get_ref(),
        user.id,
        &request_id,
        "POST",
        "/api/auth/login",
    )? {
        let status = actix_web::http::StatusCode::from_u16(existing.status_code as u16)
            .unwrap_or(actix_web::http::StatusCode::OK);
        return Ok(HttpResponse::build(status).json(existing.response_body));
    }

    record_attempt(&mut conn, &username, true, now, off)?;
    diesel::update(users::table.filter(users::id.eq(user.id)))
        .set(users::locked_until.eq::<Option<chrono::NaiveDateTime>>(None))
        .execute(&mut conn)?;

    let (session, raw) = session_svc::issue(pool.get_ref(), user.id, cfg.session_ttl_secs)?;

    let response = json!({
        "token": raw,
        "sessionId": session.id,
        "userId": user.id,
        "username": user.username,
        "displayName": user.display_name,
        "expiresAt": session.expires_at.format("%Y-%m-%dT%H:%M:%S").to_string(),
    });
    idempotency::store(
        pool.get_ref(),
        user.id,
        &request_id,
        "POST",
        "/api/auth/login",
        200,
        response.clone(),
    )?;
    Ok(HttpResponse::Ok().json(response))
}

fn record_attempt(
    conn: &mut diesel::PgConnection,
    username: &str,
    succeeded: bool,
    now: chrono::NaiveDateTime,
    off: i16,
) -> AppResult<()> {
    let row = NewLoginAttempt {
        id: Uuid::new_v4(),
        username: username.to_string(),
        succeeded,
        attempted_at: now,
        attempted_offset_minutes: off,
    };
    diesel::insert_into(login_attempts::table)
        .values(&row)
        .execute(conn)?;
    Ok(())
}

async fn logout(req: HttpRequest, pool: web::Data<DbPool>) -> AppResult<HttpResponse> {
    let ctx = {
        let ext = req.extensions();
        ext.get::<RequestContext>()
            .cloned()
            .ok_or(AppError::Unauthenticated)?
    };
    if let Some(replay) = idempotency::check_before(pool.get_ref(), &ctx, &req)? {
        return Ok(replay);
    }
    session_svc::revoke(pool.get_ref(), ctx.session_id)?;
    let response = json!({ "status": "logged_out" });
    idempotency::record_after(pool.get_ref(), &ctx, "POST", "/api/auth/logout", 200, &response)?;
    Ok(HttpResponse::Ok().json(response))
}

#[derive(Debug, Deserialize)]
struct ChangePasswordBody {
    #[serde(rename = "currentPassword")]
    current_password: String,
    #[serde(rename = "newPassword")]
    new_password: String,
}

async fn change_password(
    req: HttpRequest,
    pool: web::Data<DbPool>,
    body: web::Json<ChangePasswordBody>,
) -> AppResult<HttpResponse> {
    let ctx = {
        let ext = req.extensions();
        ext.get::<RequestContext>()
            .cloned()
            .ok_or(AppError::Unauthenticated)?
    };
    let user_id = ctx.user.id;
    let mut conn = pool.get()?;
    let user: User = users::table.filter(users::id.eq(user_id)).first(&mut conn)?;
    if !password::verify_password(&body.current_password, &user.password_hash) {
        return Err(AppError::Validation {
            message: "current password is incorrect".into(),
            details: json!({ "field": "currentPassword" }),
        });
    }
    password::validate_policy(&body.new_password)?;
    drop(conn);
    if let Some(replay) = idempotency::check_before(pool.get_ref(), &ctx, &req)? {
        return Ok(replay);
    }
    let new_hash = password::hash_password(&body.new_password)?;
    let (now, off) = now_utc_naive();
    let mut conn = pool.get()?;
    diesel::update(users::table.filter(users::id.eq(user_id)))
        .set((
            users::password_hash.eq(new_hash),
            users::updated_at.eq(now),
            users::updated_offset_minutes.eq(off),
        ))
        .execute(&mut conn)?;
    let response = json!({ "status": "password_changed" });
    idempotency::record_after(
        pool.get_ref(),
        &ctx,
        "POST",
        "/api/auth/change-password",
        200,
        &response,
    )?;
    Ok(HttpResponse::Ok().json(response))
}

async fn session_info(req: HttpRequest) -> AppResult<HttpResponse> {
    let ext = req.extensions();
    let ctx = ext
        .get::<RequestContext>()
        .ok_or(AppError::Unauthenticated)?;
    let role_names: Vec<_> = ctx.roles.iter().map(|r| r.name.clone()).collect();
    Ok(HttpResponse::Ok().json(json!({
        "userId": ctx.user.id,
        "username": ctx.user.username,
        "displayName": ctx.user.display_name,
        "roles": role_names,
        "permissions": ctx.permissions.iter().collect::<Vec<_>>(),
    })))
}
